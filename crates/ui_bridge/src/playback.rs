use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use qingyin_metadata::TrackMetadata;
use qingyin_player::{PlaybackState, Player, PlayerEvent};
use qmetaobject::prelude::*;

#[derive(Debug)]
struct PendingPlaybackUi {
    generation: u64,
    title: String,
    artist: String,
    cover: String,
    duration: i64,
}

/// Load/play/EOS/progress/volume, bound by `PlayerBar` instead of the session object.
#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct PlaybackController {
    base: qt_base_class!(trait QObject),
    player: Option<Player>,
    /// Playback session order; not replaced when the visible list is filtered or sorted.
    playback_tracks: Vec<TrackMetadata>,
    playback_cover_urls: Vec<String>,
    current_index: Option<usize>,
    /// True while a `playbin` load/play is in flight; extra next/prev wait.
    play_in_flight: bool,
    queued_play_row: Option<i32>,
    playback_ui_generation: u64,
    persist_volume: Option<Arc<dyn Fn(f64) + Send + Sync>>,
    playback_state: qt_property!(QString; NOTIFY playback_changed),
    current_title: qt_property!(QString; NOTIFY playback_changed),
    current_artist: qt_property!(QString; NOTIFY playback_changed),
    current_cover: qt_property!(QString; NOTIFY playback_changed),
    playback_error: qt_property!(QString; NOTIFY playback_changed),
    playback_changed: qt_signal!(),
    playback_position: qt_property!(i64; NOTIFY playback_progress_changed),
    playback_duration: qt_property!(i64; NOTIFY playback_progress_changed),
    player_volume: qt_property!(f64; NOTIFY playback_progress_changed),
    playback_progress_changed: qt_signal!(),
    toggle_playback: qt_method!(
        fn toggle_playback(&mut self) {
            crate::pointer_trace("slot", "toggle_playback");
            self.toggle_playback_internal();
        }
    ),
    play_previous: qt_method!(
        fn play_previous(&mut self) {
            crate::pointer_trace("slot", "play_previous");
            let Some(current) = self.current_index else {
                crate::pointer_trace("slot", "play_previous skipped: no current track");
                return;
            };
            self.play_queue_row(i32::try_from(crate::previous_track_index(current)).unwrap_or(0));
        }
    ),
    play_next: qt_method!(
        fn play_next(&mut self) {
            crate::pointer_trace("slot", "play_next");
            self.play_next_internal();
        }
    ),
    seek_to: qt_method!(
        fn seek_to(&mut self, position: i64) {
            self.seek_to_internal(position);
        }
    ),
    set_player_volume: qt_method!(
        fn set_player_volume(&mut self, volume: f64) {
            self.set_player_volume_internal(volume);
        }
    ),
}

impl PlaybackController {
    pub fn set_volume_persist(&mut self, persist: impl Fn(f64) + Send + Sync + 'static) {
        self.persist_volume = Some(Arc::new(persist));
    }

    #[must_use]
    pub fn volume(&self) -> f64 {
        self.player_volume
    }

    pub fn apply_saved_volume(&mut self, volume: f64) {
        self.apply_volume(volume.clamp(0.0, 1.0));
        self.playback_progress_changed();
    }

    pub fn shutdown(&mut self) {
        self.player = None;
    }

    #[must_use]
    pub fn current_path(&self) -> Option<PathBuf> {
        self.player
            .as_ref()
            .and_then(Player::current_path)
            .map(Path::to_path_buf)
    }

    #[must_use]
    pub fn has_playback_tracks(&self) -> bool {
        !self.playback_tracks.is_empty()
    }

    pub fn set_current_index_if_idle(&mut self, position: Option<usize>) {
        if self.has_playback_tracks() {
            return;
        }
        self.current_index = position;
    }

    pub fn play_from_list(
        &mut self,
        tracks: Vec<TrackMetadata>,
        cover_urls: Vec<String>,
        row: i32,
    ) {
        self.playback_tracks = tracks;
        self.playback_cover_urls = cover_urls;
        self.play_queue_row(row);
    }

    pub fn skip_missing_current_track(&mut self) {
        let Some(path) = self.current_path() else {
            return;
        };
        if path.exists() {
            return;
        }
        let Some(next) = self
            .current_index
            .and_then(|current| crate::next_track_index(current, self.playback_tracks.len()))
        else {
            self.stop_playback_silently();
            return;
        };
        if let Ok(next) = i32::try_from(next) {
            self.play_queue_row(next);
        }
        let missing = self
            .player
            .as_ref()
            .and_then(Player::current_path)
            .is_none_or(|current| !current.exists());
        if missing {
            self.stop_playback_silently();
        }
    }

    fn ensure_player(&mut self) -> Result<(), String> {
        if self.player.is_some() {
            return Ok(());
        }

        let mut player = Player::initialize().map_err(|error| error.to_string())?;
        let controller = QPointer::from(&*self);
        let dispatch = qmetaobject::queued_callback(move |event: PlayerEvent| {
            let Some(controller) = controller.as_pinned() else {
                return;
            };
            controller.borrow_mut().handle_player_event(event);
        });
        player
            .set_event_handler(dispatch)
            .map_err(|error| error.to_string())?;
        player.set_volume(self.player_volume);
        self.player_volume = player.volume();
        self.player = Some(player);
        self.playback_progress_changed();
        Ok(())
    }

    fn play_queue_row(&mut self, row: i32) {
        if self.play_in_flight {
            self.queued_play_row = Some(row);
            return;
        }
        self.start_queue_row(row);
    }

    fn start_queue_row(&mut self, row: i32) {
        let Some((row, track)) = usize::try_from(row).ok().and_then(|row| {
            self.playback_tracks
                .get(row)
                .cloned()
                .map(|track| (row, track))
        }) else {
            return;
        };
        if let Err(error) = self.ensure_player() {
            self.set_playback_error(error);
            return;
        }
        let Some(player) = self.player.as_mut() else {
            return;
        };
        self.play_in_flight = true;
        if let Err(error) = player.load(&track.path).and_then(|()| player.play()) {
            self.play_in_flight = false;
            self.set_playback_error(error.to_string());
            return;
        }

        self.current_index = Some(row);
        self.playback_ui_generation = self.playback_ui_generation.wrapping_add(1);
        let pending = PendingPlaybackUi {
            generation: self.playback_ui_generation,
            title: track.title.clone(),
            artist: track.artists.join("、"),
            cover: self
                .playback_cover_urls
                .get(row)
                .cloned()
                .unwrap_or_default(),
            duration: crate::duration_millis(track.duration),
        };
        let controller = QPointer::from(&*self);
        let publish = qmetaobject::queued_callback(move |pending: PendingPlaybackUi| {
            let Some(controller) = controller.as_pinned() else {
                return;
            };
            controller.borrow_mut().publish_playback_ui(pending);
        });
        publish(pending);
    }

    fn publish_playback_ui(&mut self, pending: PendingPlaybackUi) {
        if pending.generation != self.playback_ui_generation {
            return;
        }
        self.current_title = pending.title.into();
        self.current_artist = pending.artist.into();
        self.current_cover = pending.cover.into();
        self.playback_state = "playing".into();
        self.playback_error = QString::default();
        self.playback_position = 0;
        self.playback_duration = pending.duration;
        self.playback_changed();
        self.playback_progress_changed();
        self.play_in_flight = false;
        if let Some(row) = self.queued_play_row.take() {
            self.start_queue_row(row);
        }
    }

    fn toggle_playback_internal(&mut self) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        let state = player.state();
        if state == PlaybackState::Stopped {
            if let Some(row) = self.current_index.and_then(|row| i32::try_from(row).ok()) {
                self.play_queue_row(row);
            }
            return;
        }

        let result = match state {
            PlaybackState::Playing => player.pause(),
            PlaybackState::Paused => player.play(),
            PlaybackState::Stopped => unreachable!(),
        };
        if let Err(error) = result {
            self.set_playback_error(error.to_string());
            return;
        }
        self.playback_state = match player.state() {
            PlaybackState::Playing => "playing".into(),
            PlaybackState::Paused => "paused".into(),
            PlaybackState::Stopped => "stopped".into(),
        };
        self.playback_error = QString::default();
        self.playback_changed();
    }

    fn play_next_internal(&mut self) {
        let Some(next) = self
            .current_index
            .and_then(|current| crate::next_track_index(current, self.playback_tracks.len()))
        else {
            if self.current_index.is_some() {
                if let Some(player) = self.player.as_mut() {
                    let _ = player.stop();
                }
                self.playback_state = "stopped".into();
                self.playback_position = 0;
                self.playback_changed();
                self.playback_progress_changed();
            }
            return;
        };
        if let Ok(next) = i32::try_from(next) {
            self.play_queue_row(next);
        }
    }

    fn seek_to_internal(&mut self, position: i64) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        let maximum = self.playback_duration.max(0);
        let position = position.clamp(0, maximum);
        let Ok(position_millis) = u64::try_from(position) else {
            return;
        };
        if let Err(error) = player.seek(Duration::from_millis(position_millis)) {
            self.set_playback_error(error.to_string());
            return;
        }
        self.playback_position = position;
        self.playback_progress_changed();
    }

    fn set_player_volume_internal(&mut self, volume: f64) {
        let volume = volume.clamp(0.0, 1.0);
        self.apply_volume(volume);
        if let Some(persist) = &self.persist_volume {
            persist(self.player_volume);
        }
        self.playback_progress_changed();
    }

    fn apply_volume(&mut self, volume: f64) {
        if let Some(player) = self.player.as_mut() {
            player.set_volume(volume);
            self.player_volume = player.volume();
        } else {
            self.player_volume = volume;
        }
    }

    fn handle_player_event(&mut self, event: PlayerEvent) {
        match event {
            PlayerEvent::EndOfStream => self.play_next_internal(),
            PlayerEvent::Error(error) => self.set_playback_error(error),
            PlayerEvent::Progress { position, duration } => {
                self.apply_playback_progress(position, duration);
            }
        }
    }

    fn apply_playback_progress(&mut self, position: Duration, duration: Option<Duration>) {
        let position = crate::duration_millis(Some(position));
        let duration = crate::duration_millis(duration);
        if position != self.playback_position
            || (duration > 0 && duration != self.playback_duration)
        {
            self.playback_position = position;
            if duration > 0 {
                self.playback_duration = duration;
            }
            self.playback_progress_changed();
        }
    }

    fn stop_playback_silently(&mut self) {
        if let Some(player) = self.player.as_mut() {
            let _ = player.stop();
        }
        self.playback_state = "stopped".into();
        self.playback_error = QString::default();
        self.playback_position = 0;
        self.playback_changed();
        self.playback_progress_changed();
    }

    fn set_playback_error(&mut self, error: String) {
        self.play_in_flight = false;
        self.queued_play_row = None;
        self.playback_ui_generation = self.playback_ui_generation.wrapping_add(1);
        self.playback_state = "stopped".into();
        self.playback_error = error.into();
        self.playback_changed();
    }
}
