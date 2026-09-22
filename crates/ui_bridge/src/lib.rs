mod collections;
mod library_session;
mod playback;

use collections::TrackListModel;
use cstr::cstr;
use library_session::{HostEvent, LibrarySession};
use playback::PlaybackController;
#[cfg(test)]
use qingyin_core::SortColumn;
use qingyin_core::{PlayMode, Settings, SettingsLoad};
#[cfg(test)]
use qingyin_library::TrackSnapshot;
use qingyin_metadata::TrackMetadata;
use qmetaobject::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::warn;

struct SettingsWriter {
    mailbox: Arc<Mutex<Option<Settings>>>,
    last_error: Arc<Mutex<Option<String>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl SettingsWriter {
    fn start() -> Self {
        let mailbox = Arc::new(Mutex::new(None));
        let last_error = Arc::new(Mutex::new(None));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_mailbox = Arc::clone(&mailbox);
        let worker_error = Arc::clone(&last_error);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("qingyin-settings".into())
            .spawn(move || {
                loop {
                    if worker_stop.load(std::sync::atomic::Ordering::Relaxed) {
                        if let Some(settings) = worker_mailbox
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .take()
                        {
                            save_settings(&settings, &worker_error);
                        }
                        break;
                    }
                    let settings = worker_mailbox
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take();
                    if let Some(settings) = settings {
                        save_settings(&settings, &worker_error);
                    }
                    thread::sleep(Duration::from_millis(200));
                }
            })
            .ok();
        Self {
            mailbox,
            last_error,
            stop,
            worker,
        }
    }

    fn enqueue(&self, settings: Settings) {
        *self
            .mailbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(settings);
    }

    fn flush(&self, settings: &Settings) {
        self.enqueue(settings.clone());
        if let Some(settings) = self
            .mailbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            save_settings(&settings, &self.last_error);
        }
    }

    fn take_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    fn shutdown(&mut self, settings: &Settings) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        self.flush(settings);
        let _ = self.worker.take();
    }
}

#[allow(missing_debug_implementations, clippy::struct_excessive_bools)]
#[derive(QObject, Default)]
pub struct AppBridge {
    base: qt_base_class!(trait QObject),
    library: qt_property!(RefCell<LibrarySession>; CONST),
    playback: qt_property!(RefCell<PlaybackController>; CONST),
    dark_theme: qt_property!(bool; NOTIFY settings_changed),
    music_folders: qt_property!(QString; NOTIFY settings_changed),
    settings_error: qt_property!(QString; NOTIFY settings_changed),
    settings_changed: qt_signal!(),
    settings: Settings,
    settings_writer: Option<SettingsWriter>,
    restored: bool,
    application_name: qt_method!(
        fn application_name(&self) -> QString {
            let _ = self;
            "清音".into()
        }
    ),
    version: qt_method!(
        fn version(&self) -> QString {
            let _ = self;
            env!("CARGO_PKG_VERSION").into()
        }
    ),
    set_dark_theme: qt_method!(
        fn set_dark_theme(&mut self, dark: bool) {
            self.set_dark_theme_internal(dark);
        }
    ),
    restore_session: qt_method!(
        fn restore_session(&mut self) {
            self.restore_session_internal();
        }
    ),
    flush_settings: qt_method!(
        fn flush_settings(&mut self) {
            self.playback.borrow_mut().flush_volume_internal();
            self.persist_settings(true);
        }
    ),
    shutdown: qt_method!(
        fn shutdown(&mut self) {
            self.shutdown_internal();
        }
    ),
}

impl AppBridge {
    fn shutdown_internal(&mut self) {
        self.library.borrow_mut().shutdown();
        self.playback.borrow_mut().shutdown();
        self.capture_settings_from_ui();
        if let Some(writer) = self.settings_writer.as_mut() {
            writer.shutdown(&self.settings);
        }
    }

    fn set_dark_theme_internal(&mut self, dark: bool) {
        if self.dark_theme == dark {
            return;
        }
        self.dark_theme = dark;
        self.settings.dark_theme = dark;
        self.persist_settings(false);
        self.settings_changed();
    }

    fn restore_session_internal(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        if self.settings_writer.is_none() {
            self.settings_writer = Some(SettingsWriter::start());
        }
        self.attach_host();
        let loaded = Settings::load_status();
        let can_prune = loaded.can_prune_library();
        match &loaded {
            SettingsLoad::Failed { error, .. } => {
                self.settings_error = error.to_string().into();
            }
            _ => self.settings_error = QString::default(),
        }
        self.settings = loaded.into_settings();
        self.apply_settings_to_ui();
        self.library.borrow_mut().restore(
            self.settings.music_directories.clone(),
            self.settings.sort_column,
            self.settings.sort_ascending,
            can_prune,
        );
    }

    fn apply_settings_to_ui(&mut self) {
        self.dark_theme = self.settings.dark_theme;
        self.playback
            .borrow_mut()
            .apply_saved_volume(self.settings.volume);
        self.playback
            .borrow_mut()
            .apply_saved_play_mode(self.settings.play_mode);
        self.music_folders = format_music_folders(&self.settings.music_directories).into();
        self.settings_changed();
    }

    fn capture_settings_from_ui(&mut self) {
        self.settings.volume = self.playback.borrow().volume();
        self.settings.play_mode = self.playback.borrow().play_mode();
        {
            let library = self.library.borrow();
            self.settings.music_directories = library.music_directories().to_vec();
            self.settings.sort_column = library.sort_column();
            self.settings.sort_ascending = library.sort_is_ascending();
        }
        self.music_folders = format_music_folders(&self.settings.music_directories).into();
    }

    fn persist_settings(&mut self, flush: bool) {
        let previous_folders = self.music_folders.clone();
        let previous_error = self.settings_error.clone();
        self.capture_settings_from_ui();
        if let Some(writer) = &self.settings_writer {
            if flush {
                writer.flush(&self.settings);
            } else {
                writer.enqueue(self.settings.clone());
            }
            if let Some(error) = writer.take_error() {
                self.settings_error = error.into();
            }
        } else if let Err(error) = self.settings.save() {
            warn!(%error, "failed to save settings");
            self.settings_error = error.to_string().into();
        }
        if previous_folders != self.music_folders || previous_error != self.settings_error {
            self.settings_changed();
        }
    }

    fn attach_host(&mut self) {
        let bridge = QPointer::from(&*self);
        let host = qmetaobject::queued_callback(move |event: HostEvent| {
            let Some(bridge) = bridge.as_pinned() else {
                return;
            };
            let mut bridge = bridge.borrow_mut();
            match event {
                HostEvent::Persist => bridge.persist_settings(false),
                HostEvent::Play { tracks, track_id } => {
                    bridge.playback.borrow_mut().play_track_id(tracks, track_id);
                }
                HostEvent::TrackRemoved => {
                    bridge.playback.borrow_mut().skip_missing_current_track();
                }
            }
        });
        self.library.borrow_mut().set_host(host);

        let persist_bridge = QPointer::from(&*self);
        let persist = qmetaobject::queued_callback(move |volume: f64| {
            let Some(bridge) = persist_bridge.as_pinned() else {
                return;
            };
            let mut bridge = bridge.borrow_mut();
            if (bridge.settings.volume - volume).abs() > f64::EPSILON {
                bridge.settings.volume = volume;
                bridge.persist_settings(false);
            }
        });
        self.playback.borrow_mut().set_volume_persist(persist);

        let mode_bridge = QPointer::from(&*self);
        let persist_mode = qmetaobject::queued_callback(move |mode: PlayMode| {
            let Some(bridge) = mode_bridge.as_pinned() else {
                return;
            };
            let mut bridge = bridge.borrow_mut();
            if bridge.settings.play_mode != mode {
                bridge.settings.play_mode = mode;
                bridge.persist_settings(false);
            }
        });
        self.playback
            .borrow_mut()
            .set_play_mode_persist(persist_mode);
    }
}

pub fn register_qml_types() {
    qml_register_type::<AppBridge>(cstr!("Qingyin"), 1, 0, cstr!("AppBridge"));
    qml_register_type::<LibrarySession>(cstr!("Qingyin"), 1, 0, cstr!("LibrarySession"));
    qml_register_type::<TrackListModel>(cstr!("Qingyin"), 1, 0, cstr!("TrackListModel"));
    qml_register_type::<PlaybackController>(cstr!("Qingyin"), 1, 0, cstr!("PlaybackController"));
}

fn save_settings(settings: &Settings, last_error: &Mutex<Option<String>>) {
    match settings.save() {
        Ok(()) => {
            *last_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }
        Err(error) => {
            warn!(%error, "failed to save settings");
            *last_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(error.to_string());
        }
    }
}

fn format_music_folders(directories: &[PathBuf]) -> String {
    directories
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
pub(crate) fn sort_snapshots(tracks: &mut [TrackSnapshot], column: SortColumn, ascending: bool) {
    tracks.sort_by(|left, right| {
        let ordering = left.cmp_column(right, column);
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
}

pub(crate) const fn previous_track_index(current: usize, track_count: usize) -> Option<usize> {
    if track_count == 0 {
        return None;
    }
    Some(if current.is_multiple_of(track_count) {
        track_count - 1
    } else {
        (current % track_count) - 1
    })
}

pub(crate) const fn next_track_index(current: usize, track_count: usize) -> Option<usize> {
    if track_count == 0 {
        return None;
    }
    Some((current % track_count + 1) % track_count)
}

pub(crate) fn format_duration(track: &TrackMetadata) -> String {
    let seconds = track.duration.map_or(0, |duration| duration.as_secs());
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub(crate) fn duration_millis(duration: Option<Duration>) -> i64 {
    duration.map_or(0, |duration| {
        i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingyin_library::TrackSnapshot;
    use std::time::Duration;

    #[test]
    fn track_navigation_wraps_the_list() {
        assert_eq!(previous_track_index(0, 0), None);
        assert_eq!(previous_track_index(0, 4), Some(3));
        assert_eq!(previous_track_index(3, 4), Some(2));
        assert_eq!(next_track_index(0, 2), Some(1));
        assert_eq!(next_track_index(1, 2), Some(0));
        assert_eq!(next_track_index(0, 0), None);
    }

    #[test]
    fn sorts_tracks_by_selected_column_and_direction() {
        let mut tracks = vec![test_track("B", "专辑甲", 20), test_track("A", "专辑乙", 10)];
        sort_snapshots(&mut tracks, SortColumn::Title, true);
        assert_eq!(tracks[0].metadata.title, "A");
        sort_snapshots(&mut tracks, SortColumn::Duration, false);
        assert_eq!(tracks[0].metadata.duration, Some(Duration::from_secs(20)));
    }

    #[test]
    fn sorts_han_titles_among_latin_titles() {
        let mut tracks = vec![
            test_track("周杰伦", "专辑", 10),
            test_track("Adele", "专辑", 10),
            test_track("阿妹", "专辑", 10),
        ];
        sort_snapshots(&mut tracks, SortColumn::Title, true);
        assert_eq!(
            tracks
                .iter()
                .map(|track| track.metadata.title.as_str())
                .collect::<Vec<_>>(),
            ["Adele", "阿妹", "周杰伦"]
        );
    }

    #[test]
    fn formats_music_folders_on_separate_lines() {
        let folders = format_music_folders(&[PathBuf::from("/music/a"), PathBuf::from("/music/b")]);
        assert_eq!(folders, "/music/a\n/music/b");
    }

    fn test_track(title: &str, album: &str, duration: u64) -> TrackSnapshot {
        TrackSnapshot::from_metadata(TrackMetadata::from_display(
            format!("/music/{title}.flac"),
            title,
            Some(album.to_owned()),
            Vec::new(),
            Some(Duration::from_secs(duration)),
        ))
    }
}
