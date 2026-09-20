mod collections;
mod library_session;
mod playback;
#[allow(dead_code)]
mod pointer_guard;

use collections::TrackListModel;
use cstr::cstr;
use library_session::LibrarySession;
use playback::PlaybackController;
use qingyin_core::Settings;
use qingyin_library::TrackSnapshot;
use qingyin_metadata::TrackMetadata;
use qmetaobject::prelude::*;
use std::cell::RefCell;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::warn;

#[allow(missing_debug_implementations, clippy::struct_excessive_bools)]
#[derive(QObject, Default)]
pub struct AppBridge {
    base: qt_base_class!(trait QObject),
    library: qt_property!(RefCell<LibrarySession>; CONST),
    playback: qt_property!(RefCell<PlaybackController>; CONST),
    dark_theme: qt_property!(bool; NOTIFY settings_changed),
    music_folders: qt_property!(QString; NOTIFY settings_changed),
    settings_changed: qt_signal!(),
    settings: Settings,
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
    pointer_debug_enabled: qt_method!(
        fn pointer_debug_enabled(&self) -> bool {
            let _ = self;
            pointer_debug_from_env()
        }
    ),
    log_pointer: qt_method!(
        fn log_pointer(&self, kind: QString, target: QString, extra: QString) {
            let _ = self;
            let kind: String = kind.into();
            let target: String = target.into();
            let extra: String = extra.into();
            pointer_trace(&kind, &format!("{target}  {extra}"));
        }
    ),
    drop_pointer_grabs: qt_method!(
        fn drop_pointer_grabs(&self) {
            let _ = self;
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
    }

    fn set_dark_theme_internal(&mut self, dark: bool) {
        if self.dark_theme == dark {
            return;
        }
        self.dark_theme = dark;
        self.settings.dark_theme = dark;
        self.persist_settings();
        self.settings_changed();
    }

    fn restore_session_internal(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        self.attach_library_host();
        self.attach_playback_persist();
        self.settings = Settings::load_or_default();
        self.apply_settings_to_ui();
        self.library.borrow_mut().restore(
            self.settings.music_directories.clone(),
            self.settings.sort_column.clone(),
            self.settings.sort_ascending,
        );
    }

    fn apply_settings_to_ui(&mut self) {
        self.dark_theme = self.settings.dark_theme;
        self.playback
            .borrow_mut()
            .apply_saved_volume(self.settings.volume);
        self.music_folders = format_music_folders(&self.settings.music_directories).into();
        self.settings_changed();
    }

    fn persist_settings(&mut self) {
        self.settings.volume = self.playback.borrow().volume();
        {
            let library = self.library.borrow();
            self.settings.music_directories = library.music_directories().to_vec();
            self.settings.sort_column = library.sort_column_string();
            self.settings.sort_ascending = library.sort_is_ascending();
        }
        self.music_folders = format_music_folders(&self.settings.music_directories).into();
        self.settings_changed();
        if let Err(error) = self.settings.save() {
            warn!(%error, "failed to save settings");
        }
    }

    fn attach_playback_persist(&mut self) {
        let bridge = QPointer::from(&*self);
        let persist = qmetaobject::queued_callback(move |volume: f64| {
            let Some(bridge) = bridge.as_pinned() else {
                return;
            };
            let mut bridge = bridge.borrow_mut();
            if (bridge.settings.volume - volume).abs() > f64::EPSILON {
                bridge.settings.volume = volume;
                bridge.persist_settings();
            }
        });
        self.playback.borrow_mut().set_volume_persist(persist);
    }

    fn attach_library_host(&mut self) {
        let persist_bridge = QPointer::from(&*self);
        let persist = qmetaobject::queued_callback(move |(): ()| {
            let Some(bridge) = persist_bridge.as_pinned() else {
                return;
            };
            bridge.borrow_mut().persist_settings();
        });
        self.library.borrow_mut().set_persist(move || persist(()));

        let path_bridge = QPointer::from(&*self);
        self.library.borrow_mut().set_current_path(move || {
            path_bridge
                .as_pinned()
                .and_then(|bridge| bridge.borrow().playback.borrow().current_path())
        });

        let idle_bridge = QPointer::from(&*self);
        self.library.borrow_mut().set_idle_index(move |position| {
            let Some(bridge) = idle_bridge.as_pinned() else {
                return;
            };
            bridge
                .borrow_mut()
                .playback
                .borrow_mut()
                .set_current_index_if_idle(position);
        });

        let skip_bridge = QPointer::from(&*self);
        self.library.borrow_mut().set_skip_missing(move || {
            let Some(bridge) = skip_bridge.as_pinned() else {
                return;
            };
            bridge
                .borrow_mut()
                .playback
                .borrow_mut()
                .skip_missing_current_track();
        });

        let play_bridge = QPointer::from(&*self);
        self.library
            .borrow_mut()
            .set_play_from_list(move |tracks, covers, row| {
                let Some(bridge) = play_bridge.as_pinned() else {
                    return;
                };
                bridge
                    .borrow_mut()
                    .playback
                    .borrow_mut()
                    .play_from_list(tracks, covers, row);
            });
    }
}

pub fn register_qml_types() {
    qml_register_type::<AppBridge>(cstr!("Qingyin"), 1, 0, cstr!("AppBridge"));
    qml_register_type::<LibrarySession>(cstr!("Qingyin"), 1, 0, cstr!("LibrarySession"));
    qml_register_type::<TrackListModel>(cstr!("Qingyin"), 1, 0, cstr!("TrackListModel"));
    qml_register_type::<PlaybackController>(cstr!("Qingyin"), 1, 0, cstr!("PlaybackController"));
}

fn pointer_log_path() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| std::env::temp_dir().join("qingyin-pointer.log"))
}

fn pointer_debug_from_env() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let enabled = match std::env::var("QINGYIN_POINTER_DEBUG") {
            Ok(value) => matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
            Err(_) => false,
        };
        if enabled {
            eprintln!("[qingyin-pointer] writing {}", pointer_log_path().display());
        }
        enabled
    })
}

pub(crate) fn pointer_trace(kind: &str, detail: &str) {
    if !pointer_debug_from_env() {
        return;
    }
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis();
    let line = format!("[qingyin-pointer] {millis}  {kind}  {detail}");
    eprintln!("{line}");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(pointer_log_path())
    {
        let _ = std::io::Write::write_all(&mut file, format!("{line}\n").as_bytes());
    }
}

fn format_music_folders(directories: &[PathBuf]) -> String {
    directories
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn sort_tracks(
    tracks: &mut Vec<TrackSnapshot>,
    cover_urls: &mut Vec<String>,
    column: &str,
    ascending: bool,
) {
    let ascending = column.is_empty() || ascending;
    cover_urls.resize(tracks.len(), String::new());
    let mut rows = std::mem::take(tracks)
        .into_iter()
        .zip(std::mem::take(cover_urls))
        .collect::<Vec<_>>();
    rows.sort_by(|(left, _), (right, _)| {
        let ordering = left.cmp_column(right, column);
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
    let (sorted_tracks, sorted_covers) = rows.into_iter().unzip();
    *tracks = sorted_tracks;
    *cover_urls = sorted_covers;
}

pub(crate) const fn previous_track_index(current: usize) -> usize {
    current.saturating_sub(1)
}

pub(crate) const fn next_track_index(current: usize, track_count: usize) -> Option<usize> {
    match current.checked_add(1) {
        Some(next) if next < track_count => Some(next),
        _ => None,
    }
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

    #[test]
    fn track_navigation_stops_at_list_boundaries() {
        assert_eq!(previous_track_index(0), 0);
        assert_eq!(previous_track_index(3), 2);
        assert_eq!(next_track_index(0, 2), Some(1));
        assert_eq!(next_track_index(1, 2), None);
        assert_eq!(next_track_index(usize::MAX, usize::MAX), None);
    }

    #[test]
    fn sorts_tracks_by_selected_column_and_direction() {
        let mut tracks = vec![test_track("B", "专辑甲", 20), test_track("A", "专辑乙", 10)];
        let mut covers = vec!["B封面".to_owned(), "A封面".to_owned()];

        sort_tracks(&mut tracks, &mut covers, "title", true);
        assert_eq!(tracks[0].metadata.title, "A");
        assert_eq!(tracks[1].metadata.title, "B");
        assert_eq!(covers, ["A封面", "B封面"]);

        sort_tracks(&mut tracks, &mut covers, "duration", false);
        assert_eq!(tracks[0].metadata.duration, Some(Duration::from_secs(20)));
        assert_eq!(tracks[1].metadata.duration, Some(Duration::from_secs(10)));
        assert_eq!(covers, ["B封面", "A封面"]);
    }

    #[test]
    fn sorts_han_titles_among_latin_titles() {
        let mut tracks = vec![
            test_track("周杰伦", "专辑", 10),
            test_track("Adele", "专辑", 10),
            test_track("阿妹", "专辑", 10),
        ];
        let mut covers = vec!["z".into(), "a".into(), "m".into()];
        sort_tracks(&mut tracks, &mut covers, "title", true);
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
