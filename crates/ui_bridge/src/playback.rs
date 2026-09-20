use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use qingyin_library::TrackSnapshot;
use qingyin_player::{
    CommandId, CommandKind, FakeHandle, LoadGeneration, PlaybackState, Player, PlayerEvent,
};
use qmetaobject::prelude::*;

const MAX_MISSING_SKIPS: usize = 10_000;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NotifySet {
    pub identity: bool,
    pub position: bool,
    pub duration: bool,
    pub volume: bool,
}

impl NotifySet {
    fn identity() -> Self {
        Self {
            identity: true,
            ..Self::default()
        }
    }
}

/// Qt-free playback session used by the controller and by fake-backend tests.
#[derive(Debug)]
pub(crate) struct PlaybackEngine {
    pub requested: PlaybackState,
    pub confirmed: PlaybackState,
    pub generation: LoadGeneration,
    pub ready: bool,
    pub queue: Vec<TrackSnapshot>,
    pub current: Option<usize>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub displayed_second: i64,
    pub volume: f64,
    pub title: String,
    pub artist: String,
    pub cover: String,
    pub error: String,
    pub pending_row: Option<usize>,
    pending_command: Option<(CommandId, CommandKind)>,
}

impl Default for PlaybackEngine {
    fn default() -> Self {
        Self {
            requested: PlaybackState::Stopped,
            confirmed: PlaybackState::Stopped,
            generation: 0,
            ready: false,
            queue: Vec::new(),
            current: None,
            position_ms: 0,
            duration_ms: 0,
            displayed_second: 0,
            volume: 1.0,
            title: String::new(),
            artist: String::new(),
            cover: String::new(),
            error: String::new(),
            pending_row: None,
            pending_command: None,
        }
    }
}

impl PlaybackEngine {
    pub fn handle_event(&mut self, event: PlayerEvent) -> NotifySet {
        match event {
            PlayerEvent::Ready => {
                self.ready = true;
                NotifySet::identity()
            }
            PlayerEvent::CommandFinished { id, kind, error } => {
                if self
                    .pending_command
                    .is_some_and(|(pending, pending_kind)| pending == id && pending_kind == kind)
                {
                    self.pending_command = None;
                }
                if let Some(error) = error {
                    self.apply_failure(error)
                } else {
                    NotifySet::default()
                }
            }
            PlayerEvent::StateChanged { generation, state } => {
                if generation != self.generation {
                    return NotifySet::default();
                }
                self.confirmed = state;
                NotifySet::identity()
            }
            PlayerEvent::Progress {
                generation,
                position,
                duration,
            } => {
                if generation != self.generation {
                    return NotifySet::default();
                }
                self.apply_progress(position, duration)
            }
            PlayerEvent::EndOfStream { generation } => {
                if generation != self.generation {
                    return NotifySet::default();
                }
                NotifySet::identity()
            }
            PlayerEvent::Error {
                generation,
                message,
            } => {
                if generation != self.generation {
                    return NotifySet::default();
                }
                self.apply_failure(message)
            }
            PlayerEvent::ShutdownFinished => NotifySet::default(),
        }
    }

    pub fn apply_progress(&mut self, position: Duration, duration: Option<Duration>) -> NotifySet {
        let position_ms = crate::duration_millis(Some(position));
        let duration_ms = crate::duration_millis(duration);
        let second = position_ms / 1000;
        let mut notify = NotifySet::default();
        if duration_ms > 0 && duration_ms != self.duration_ms {
            self.duration_ms = duration_ms;
            notify.duration = true;
        }
        if position_ms != self.position_ms {
            self.position_ms = position_ms;
            if second != self.displayed_second {
                self.displayed_second = second;
            }
            notify.position = true;
        }
        notify
    }

    pub fn apply_failure(&mut self, error: String) -> NotifySet {
        self.requested = PlaybackState::Stopped;
        self.confirmed = PlaybackState::Stopped;
        self.position_ms = 0;
        self.displayed_second = 0;
        self.error = error;
        NotifySet {
            identity: true,
            position: true,
            ..NotifySet::default()
        }
    }

    pub fn publish_current(&mut self) {
        let Some(index) = self.current else {
            return;
        };
        let Some(track) = self.queue.get(index) else {
            return;
        };
        self.title = track.metadata.title.clone();
        self.artist = track.metadata.artists.join("、");
        self.cover = track.cover_url.clone();
        self.error.clear();
        self.position_ms = 0;
        self.displayed_second = 0;
        self.duration_ms = crate::duration_millis(track.metadata.duration);
    }

    pub fn skip_missing(&mut self, exists: impl Fn(&Path) -> bool) -> Option<usize> {
        let start = self.current?;
        let len = self.queue.len();
        if len == 0 {
            return None;
        }
        let limit = len.min(MAX_MISSING_SKIPS);
        for offset in 1..=limit {
            let index = (start + offset) % len;
            if exists(&self.queue[index].metadata.path) {
                return Some(index);
            }
            if offset == len {
                break;
            }
        }
        None
    }

    pub fn previous_index(&self) -> Option<usize> {
        self.current
            .filter(|index| *index > 0)
            .map(crate::previous_track_index)
    }

    pub fn next_index(&self) -> Option<usize> {
        self.current
            .and_then(|current| crate::next_track_index(current, self.queue.len()))
    }
}

/// Load/play/EOS/progress/volume, bound by `PlayerBar`.
#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct PlaybackController {
    base: qt_base_class!(trait QObject),
    player: Option<Player>,
    fake: Option<FakeHandle>,
    engine: PlaybackEngine,
    persist_volume: Option<Arc<dyn Fn(f64) + Send + Sync>>,
    playback_state: qt_property!(QString; NOTIFY playback_changed),
    current_title: qt_property!(QString; NOTIFY playback_changed),
    current_artist: qt_property!(QString; NOTIFY playback_changed),
    current_cover: qt_property!(QString; NOTIFY playback_changed),
    playback_error: qt_property!(QString; NOTIFY playback_changed),
    playback_changed: qt_signal!(),
    playback_position: qt_property!(i64; NOTIFY playback_position_changed),
    playback_position_changed: qt_signal!(),
    playback_duration: qt_property!(i64; NOTIFY playback_duration_changed),
    playback_duration_changed: qt_signal!(),
    player_volume: qt_property!(f64; NOTIFY player_volume_changed),
    player_volume_changed: qt_signal!(),
    toggle_playback: qt_method!(
        fn toggle_playback(&mut self) {
            self.toggle_playback_internal();
        }
    ),
    play_previous: qt_method!(
        fn play_previous(&mut self) {
            if let Some(index) = self.engine.previous_index() {
                self.play_queue_index(index);
            }
        }
    ),
    play_next: qt_method!(
        fn play_next(&mut self) {
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
            self.set_player_volume_internal(volume, false);
        }
    ),
    flush_volume: qt_method!(
        fn flush_volume(&mut self) {
            self.flush_volume_internal();
        }
    ),
    set_ui_visible: qt_method!(
        fn set_ui_visible(&mut self, visible: bool) {
            if let Some(player) = self.player.as_mut() {
                player.set_ui_visible(visible);
            }
        }
    ),
}

impl PlaybackController {
    pub fn set_volume_persist(&mut self, persist: impl Fn(f64) + Send + Sync + 'static) {
        self.persist_volume = Some(Arc::new(persist));
    }

    #[must_use]
    pub fn volume(&self) -> f64 {
        self.engine.volume
    }

    pub fn apply_saved_volume(&mut self, volume: f64) {
        self.engine.volume = volume.clamp(0.0, 1.0);
        self.player_volume = self.engine.volume;
        self.player_volume_changed();
        if let Some(player) = self.player.as_mut() {
            let _ = player.set_volume(self.engine.volume);
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(mut player) = self.player.take() {
            let _ = player.shutdown();
        }
        self.fake = None;
    }

    pub fn play_track_id(&mut self, tracks: Vec<TrackSnapshot>, track_id: i64) {
        self.engine.queue = tracks;
        if let Some(index) = self
            .engine
            .queue
            .iter()
            .position(|track| track.id() == track_id)
        {
            self.play_queue_index(index);
        }
    }

    pub fn skip_missing_current_track(&mut self) {
        let exists = |path: &Path| path.is_file();
        match self.engine.skip_missing(exists) {
            Some(index) => self.play_queue_index(index),
            None => self.stop_playback_silently(),
        }
    }

    fn ensure_player(&mut self) {
        if self.player.is_some() {
            return;
        }
        let controller = QPointer::from(&*self);
        let dispatch = qmetaobject::queued_callback(move |event: PlayerEvent| {
            let Some(controller) = controller.as_pinned() else {
                return;
            };
            controller.borrow_mut().handle_player_event(event);
        });
        let player = Player::spawn(dispatch);
        self.player = Some(player);
    }

    fn play_queue_index(&mut self, index: usize) {
        self.ensure_player();
        if !self.engine.ready {
            self.engine.pending_row = Some(index);
            return;
        }
        let Some(track) = self.engine.queue.get(index).cloned() else {
            return;
        };
        let Some(player) = self.player.as_mut() else {
            return;
        };
        let load_id = player.load(&track.metadata.path);
        self.engine.pending_command = Some((load_id, CommandKind::Load));
        self.engine.generation = player.generation();
        self.engine.current = Some(index);
        self.engine.requested = PlaybackState::Playing;
        self.engine.publish_current();
        let play_id = player.play();
        self.engine.pending_command = Some((play_id, CommandKind::Play));
        self.publish_identity();
        self.publish_position();
        self.publish_duration();
    }

    fn toggle_playback_internal(&mut self) {
        self.ensure_player();
        let Some(player) = self.player.as_mut() else {
            return;
        };
        if !self.engine.ready {
            return;
        }
        if self.engine.confirmed == PlaybackState::Stopped {
            if let Some(index) = self.engine.current {
                self.play_queue_index(index);
            }
            return;
        }
        let id = if self.engine.confirmed == PlaybackState::Playing {
            self.engine.requested = PlaybackState::Paused;
            player.pause()
        } else {
            self.engine.requested = PlaybackState::Playing;
            player.play()
        };
        self.engine.pending_command = Some((
            id,
            if self.engine.requested == PlaybackState::Paused {
                CommandKind::Pause
            } else {
                CommandKind::Play
            },
        ));
        self.publish_identity();
    }

    fn play_next_internal(&mut self) {
        match self.engine.next_index() {
            Some(index) => self.play_queue_index(index),
            None => self.stop_playback_silently(),
        }
    }

    fn seek_to_internal(&mut self, position: i64) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        let maximum = self.engine.duration_ms.max(0);
        let position = position.clamp(0, maximum);
        let Ok(position_millis) = u64::try_from(position) else {
            return;
        };
        let id = player.seek(Duration::from_millis(position_millis));
        self.engine.pending_command = Some((id, CommandKind::Seek));
        self.engine.position_ms = position;
        self.publish_position();
    }

    fn set_player_volume_internal(&mut self, volume: f64, flush: bool) {
        let volume = volume.clamp(0.0, 1.0);
        self.engine.volume = volume;
        if let Some(player) = self.player.as_mut() {
            let _ = player.set_volume(volume);
        }
        self.player_volume = volume;
        self.player_volume_changed();
        if flush && let Some(persist) = &self.persist_volume {
            persist(volume);
        }
    }

    pub(crate) fn flush_volume_internal(&mut self) {
        if let Some(persist) = &self.persist_volume {
            persist(self.engine.volume);
        }
    }

    fn handle_player_event(&mut self, event: PlayerEvent) {
        let eos = matches!(event, PlayerEvent::EndOfStream { generation } if generation == self.engine.generation);
        let notify = self.engine.handle_event(event);
        if self.engine.ready
            && let Some(index) = self.engine.pending_row.take()
        {
            self.play_queue_index(index);
            return;
        }
        if let Some(player) = self.player.as_mut() {
            player.apply_confirmed_state(self.engine.generation, self.engine.confirmed);
            if self.engine.ready {
                player.mark_ready();
            }
        }
        self.apply_notify(notify);
        if eos {
            self.play_next_internal();
        }
    }

    fn apply_notify(&mut self, notify: NotifySet) {
        if notify.identity {
            self.publish_identity();
        }
        if notify.position {
            self.publish_position();
        }
        if notify.duration {
            self.publish_duration();
        }
        if notify.volume {
            self.player_volume = self.engine.volume;
            self.player_volume_changed();
        }
    }

    fn publish_identity(&mut self) {
        self.current_title = self.engine.title.clone().into();
        self.current_artist = self.engine.artist.clone().into();
        self.current_cover = self.engine.cover.clone().into();
        self.playback_error = self.engine.error.clone().into();
        self.playback_state = match self.engine.confirmed {
            PlaybackState::Playing => "playing".into(),
            PlaybackState::Paused => "paused".into(),
            PlaybackState::Stopped => "stopped".into(),
        };
        self.playback_changed();
    }

    fn publish_position(&mut self) {
        self.playback_position = self.engine.position_ms;
        self.playback_position_changed();
    }

    fn publish_duration(&mut self) {
        self.playback_duration = self.engine.duration_ms;
        self.playback_duration_changed();
    }

    fn stop_playback_silently(&mut self) {
        if let Some(player) = self.player.as_mut() {
            let _ = player.stop();
        }
        self.engine.requested = PlaybackState::Stopped;
        self.engine.confirmed = PlaybackState::Stopped;
        self.engine.position_ms = 0;
        self.engine.error.clear();
        self.publish_identity();
        self.publish_position();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingyin_metadata::TrackMetadata;
    use std::sync::mpsc;

    fn snapshot(title: &str, path: &str) -> TrackSnapshot {
        TrackSnapshot::from_metadata(TrackMetadata::from_display(
            path,
            title,
            None,
            Vec::new(),
            Some(Duration::from_secs(10)),
        ))
    }

    #[test]
    fn stale_progress_and_eos_do_not_affect_current_generation() {
        let mut engine = PlaybackEngine::default();
        engine.generation = 3;
        engine.handle_event(PlayerEvent::Progress {
            generation: 1,
            position: Duration::from_secs(9),
            duration: Some(Duration::from_secs(10)),
        });
        assert_eq!(engine.position_ms, 0);
        engine.handle_event(PlayerEvent::Error {
            generation: 2,
            message: "old".into(),
        });
        assert!(engine.error.is_empty());
        let notify = engine.handle_event(PlayerEvent::EndOfStream { generation: 3 });
        assert!(notify.identity);
    }

    #[test]
    fn load_failure_clears_progress_and_stops() {
        let mut engine = PlaybackEngine::default();
        engine.generation = 4;
        engine.position_ms = 1200;
        let notify = engine.handle_event(PlayerEvent::Error {
            generation: 4,
            message: "missing decoder".into(),
        });
        assert_eq!(engine.confirmed, PlaybackState::Stopped);
        assert_eq!(engine.position_ms, 0);
        assert_eq!(engine.error, "missing decoder");
        assert!(notify.identity && notify.position);
    }

    #[test]
    fn missing_skip_is_bounded_and_stops_when_all_fail() {
        let mut engine = PlaybackEngine::default();
        engine.queue = vec![
            snapshot("a", "/missing/a.flac"),
            snapshot("b", "/missing/b.flac"),
            snapshot("c", "/missing/c.flac"),
        ];
        engine.current = Some(0);
        assert!(engine.skip_missing(|_| false).is_none());
        assert_eq!(
            engine.skip_missing(|path| path.ends_with("c.flac")),
            Some(2)
        );
    }

    #[test]
    fn command_failures_are_observable_on_the_fake_backend() {
        let (tx, rx) = mpsc::channel();
        let (mut player, fake) = Player::spawn_fake(move |event| {
            let _ = tx.send(event);
        });
        loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(PlayerEvent::Ready) => break,
                Ok(_) => {}
                Err(_) => panic!("ready"),
            }
        }
        player.mark_ready();
        fake.fail_next(CommandKind::Seek, "seek exploded");
        let id = player.seek(Duration::from_secs(1));
        let event = loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(event @ PlayerEvent::CommandFinished { .. }) => break event,
                Ok(_) => {}
                Err(_) => panic!("command"),
            }
        };
        match event {
            PlayerEvent::CommandFinished {
                id: finished,
                kind: CommandKind::Seek,
                error: Some(error),
            } => {
                assert_eq!(finished, id);
                assert!(error.contains("seek exploded"));
            }
            other => panic!("{other:?}"),
        }
        let _ = player.shutdown();
    }
}
