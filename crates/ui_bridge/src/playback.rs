use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use qingyin_core::PlayMode;
use qingyin_library::TrackSnapshot;
use qingyin_player::{
    CommandId, CommandKind, FakeHandle, LoadGeneration, PlaybackState, Player, PlayerEvent,
};
use qmetaobject::prelude::*;
use qmetaobject::{QVariantList, QVariantMap};

use crate::collections::{track_role_data, track_role_names};
use rand::seq::SliceRandom;
use rand::{Rng, rng};

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
    pub mode: PlayMode,
    pub shuffle_order: Vec<usize>,
    pub shuffle_index: usize,
    pending_commands: Vec<(CommandId, CommandKind)>,
    load_failed: bool,
    stop_after_load_failure: bool,
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
            mode: PlayMode::Sequential,
            shuffle_order: Vec::new(),
            shuffle_index: 0,
            pending_commands: Vec::new(),
            load_failed: false,
            stop_after_load_failure: false,
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
                let tracked = self.take_pending(id, kind);
                if kind == CommandKind::Load && error.is_some() {
                    self.load_failed = true;
                    self.stop_after_load_failure = true;
                }
                if tracked && let Some(error) = error {
                    return self.apply_failure(error);
                }
                NotifySet::default()
            }
            PlayerEvent::StateChanged { generation, state } => {
                if generation != self.generation {
                    return NotifySet::default();
                }
                if self.load_failed && state != PlaybackState::Stopped {
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

    pub fn note_pending(&mut self, id: CommandId, kind: CommandKind) {
        self.pending_commands.push((id, kind));
    }

    fn take_pending(&mut self, id: CommandId, kind: CommandKind) -> bool {
        let before = self.pending_commands.len();
        self.pending_commands
            .retain(|(pending, pending_kind)| !(*pending == id && *pending_kind == kind));
        self.pending_commands.len() != before
    }

    pub fn take_stop_after_load_failure(&mut self) -> bool {
        let stop = self.stop_after_load_failure;
        self.stop_after_load_failure = false;
        stop
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
        self.ensure_shuffle_order();
        let start = self.current?;
        let order = self.navigation_order();
        let len = order.len();
        if len == 0 {
            return None;
        }
        let start_pos = order.iter().position(|&index| index == start).unwrap_or(0);
        let limit = len.min(MAX_MISSING_SKIPS);
        for offset in 1..=limit {
            let position = (start_pos + offset) % len;
            let index = order[position];
            if exists(&self.queue[index].metadata.path) {
                if self.mode == PlayMode::Shuffle {
                    self.shuffle_index = position;
                }
                return Some(index);
            }
            if offset == len {
                break;
            }
        }
        None
    }

    pub fn previous_index(&mut self) -> Option<usize> {
        self.ensure_shuffle_order();
        match self.mode {
            PlayMode::Shuffle => {
                let len = self.shuffle_order.len();
                if len == 0 {
                    return None;
                }
                let previous = if self.shuffle_index == 0 {
                    len - 1
                } else {
                    self.shuffle_index - 1
                };
                Some(self.shuffle_order[previous])
            }
            PlayMode::Sequential | PlayMode::RepeatOne => {
                crate::previous_track_index(self.current?, self.queue.len())
            }
        }
    }

    pub fn next_index(&mut self) -> Option<usize> {
        self.ensure_shuffle_order();
        match self.mode {
            PlayMode::Shuffle => self.next_shuffle_index(),
            PlayMode::Sequential | PlayMode::RepeatOne => {
                crate::next_track_index(self.current?, self.queue.len())
            }
        }
    }

    pub fn eos_index(&mut self) -> Option<usize> {
        match self.mode {
            PlayMode::RepeatOne => self.current,
            PlayMode::Sequential | PlayMode::Shuffle => self.next_index(),
        }
    }

    pub fn set_mode(&mut self, mode: PlayMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        if mode == PlayMode::Shuffle
            && let Some(current) = self.current
        {
            self.reshuffle_starting_at(current);
        }
    }

    pub fn remove_at(&mut self, index: usize) -> Option<bool> {
        if index >= self.queue.len() {
            return None;
        }
        let removed_current = self.current == Some(index);
        self.queue.remove(index);
        self.current = match self.current {
            Some(current) if current == index => {
                if self.queue.is_empty() {
                    None
                } else if index < self.queue.len() {
                    Some(index)
                } else {
                    Some(self.queue.len() - 1)
                }
            }
            Some(current) if current > index => Some(current - 1),
            other => other,
        };
        self.remap_shuffle_after_remove(index);
        if self.current.is_none() {
            self.title.clear();
            self.artist.clear();
            self.cover.clear();
            self.duration_ms = 0;
            self.position_ms = 0;
            self.displayed_second = 0;
        }
        Some(removed_current)
    }

    /// Order rows will actually play. Shuffle uses the shuffled walk, not the source list.
    pub fn playback_order(&mut self) -> Vec<usize> {
        self.ensure_shuffle_order();
        self.navigation_order()
    }

    pub fn current_playback_row(&mut self) -> Option<usize> {
        let current = self.current?;
        self.playback_order()
            .into_iter()
            .position(|index| index == current)
    }

    pub fn queue_index_at_playback_row(&mut self, row: usize) -> Option<usize> {
        self.playback_order().get(row).copied()
    }

    /// Removes a row from the order the listener sees. The following row becomes current.
    pub fn remove_playback_row(&mut self, row: usize) -> Option<bool> {
        let order = self.playback_order();
        let queue_index = *order.get(row)?;
        let removed_current = self.current == Some(queue_index);
        let successor = if removed_current {
            order.get(row + 1).copied().or_else(|| {
                if row > 0 {
                    order.get(row - 1).copied()
                } else {
                    None
                }
            })
        } else {
            None
        };
        self.remove_at(queue_index)?;
        if removed_current {
            self.current = successor.and_then(|previous| {
                if previous == queue_index {
                    None
                } else if previous > queue_index {
                    Some(previous - 1)
                } else {
                    Some(previous)
                }
            });
            if let Some(current) = self.current {
                self.sync_shuffle_index(current);
            } else {
                self.title.clear();
                self.artist.clear();
                self.cover.clear();
                self.duration_ms = 0;
                self.position_ms = 0;
                self.displayed_second = 0;
            }
        }
        Some(removed_current)
    }

    /// Reorders the sequence that will play. In shuffle mode the source list stays put.
    pub fn move_playback_row(&mut self, from: usize, to: usize) -> bool {
        if self.mode == PlayMode::Shuffle {
            self.ensure_shuffle_order();
            let len = self.shuffle_order.len();
            if from == to || from >= len || to >= len {
                return false;
            }
            let index = self.shuffle_order.remove(from);
            self.shuffle_order.insert(to, index);
            if let Some(current) = self.current {
                self.sync_shuffle_index(current);
            }
            return true;
        }
        self.move_item(from, to)
    }

    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        let len = self.queue.len();
        if from == to || from >= len || to >= len {
            return false;
        }
        let track = self.queue.remove(from);
        self.queue.insert(to, track);
        self.current = self.current.map(|index| remap_moved_index(index, from, to));
        self.shuffle_order = self
            .shuffle_order
            .iter()
            .copied()
            .map(|index| remap_moved_index(index, from, to))
            .collect();
        if let Some(current) = self.current {
            self.sync_shuffle_index(current);
        }
        true
    }

    fn remap_shuffle_after_remove(&mut self, removed: usize) {
        let mut order = Vec::with_capacity(self.shuffle_order.len());
        for index in self.shuffle_order.iter().copied() {
            if index == removed {
                continue;
            }
            order.push(if index > removed { index - 1 } else { index });
        }
        self.shuffle_order = order;
        if let Some(current) = self.current {
            self.sync_shuffle_index(current);
        }
    }

    pub fn prepare_queue(&mut self, current: usize) {
        self.current = Some(current);
        if self.mode == PlayMode::Shuffle {
            self.reshuffle_starting_at(current);
        } else {
            self.shuffle_order.clear();
            self.shuffle_index = 0;
        }
    }

    pub fn sync_shuffle_index(&mut self, current: usize) {
        if self.mode != PlayMode::Shuffle {
            return;
        }
        if let Some(position) = self
            .shuffle_order
            .iter()
            .position(|&index| index == current)
        {
            self.shuffle_index = position;
            return;
        }
        self.reshuffle_starting_at(current);
    }

    fn next_shuffle_index(&mut self) -> Option<usize> {
        let len = self.shuffle_order.len();
        if len == 0 {
            return None;
        }
        if self.shuffle_index + 1 < len {
            return Some(self.shuffle_order[self.shuffle_index + 1]);
        }
        let last = self.shuffle_order[self.shuffle_index];
        self.reshuffle_avoiding(last);
        self.shuffle_order.first().copied()
    }

    fn navigation_order(&self) -> Vec<usize> {
        if self.mode == PlayMode::Shuffle && self.shuffle_order.len() == self.queue.len() {
            self.shuffle_order.clone()
        } else {
            (0..self.queue.len()).collect()
        }
    }

    fn ensure_shuffle_order(&mut self) {
        if self.mode != PlayMode::Shuffle {
            return;
        }
        let len = self.queue.len();
        let current_in_order = self
            .current
            .is_some_and(|current| self.shuffle_order.contains(&current));
        if self.shuffle_order.len() == len && current_in_order {
            if let Some(current) = self.current
                && let Some(position) = self
                    .shuffle_order
                    .iter()
                    .position(|&index| index == current)
            {
                self.shuffle_index = position;
            }
            return;
        }
        match self.current {
            Some(current) => self.reshuffle_starting_at(current),
            None if len > 0 => self.reshuffle_starting_at(0),
            None => {
                self.shuffle_order.clear();
                self.shuffle_index = 0;
            }
        }
    }

    fn reshuffle_starting_at(&mut self, first: usize) {
        self.shuffle_order = shuffle_order_from(self.queue.len(), first, &mut rng());
        self.shuffle_index = 0;
    }

    fn reshuffle_avoiding(&mut self, avoid: usize) {
        self.shuffle_order = shuffle_order_avoiding(self.queue.len(), avoid, &mut rng());
        self.shuffle_index = 0;
    }
}

fn shuffle_order_from<R: Rng + ?Sized>(len: usize, first: usize, rng: &mut R) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let first = first.min(len - 1);
    let mut rest: Vec<usize> = (0..len).filter(|&index| index != first).collect();
    rest.shuffle(rng);
    std::iter::once(first).chain(rest).collect()
}

fn remap_moved_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        return to;
    }
    if from < to && index > from && index <= to {
        return index - 1;
    }
    if to < from && index >= to && index < from {
        return index + 1;
    }
    index
}

fn shuffle_order_avoiding<R: Rng + ?Sized>(len: usize, avoid: usize, rng: &mut R) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..len).collect();
    order.shuffle(rng);
    if len > 1 && order[0] == avoid {
        order.swap(0, 1);
    }
    order
}

/// Tracks in the order they will play. Shuffle mode stores that walk here.
#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct QueueModel {
    base: qt_base_class!(trait QAbstractListModel),
    tracks: Vec<TrackSnapshot>,
}

impl QueueModel {
    pub fn reset(&mut self, tracks: Vec<TrackSnapshot>) {
        self.begin_reset_model();
        self.tracks = tracks;
        self.end_reset_model();
    }
}

impl QAbstractListModel for QueueModel {
    fn row_count(&self) -> i32 {
        i32::try_from(self.tracks.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Ok(row) = usize::try_from(index.row()) else {
            return QVariant::default();
        };
        let Some(track) = self.tracks.get(row) else {
            return QVariant::default();
        };
        track_role_data(&track.metadata, &track.cover_url, role)
    }

    fn role_names(&self) -> std::collections::HashMap<i32, qmetaobject::QByteArray> {
        track_role_names()
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
    queue_model: qt_property!(RefCell<QueueModel>; CONST),
    current_queue_index: qt_property!(i32; NOTIFY playback_changed),
    persist_volume: Option<Arc<dyn Fn(f64) + Send + Sync>>,
    persist_play_mode: Option<Arc<dyn Fn(PlayMode) + Send + Sync>>,
    playback_state: qt_property!(QString; NOTIFY playback_changed),
    current_track_id: qt_property!(i64; NOTIFY playback_changed),
    current_title: qt_property!(QString; NOTIFY playback_changed),
    current_artist: qt_property!(QString; NOTIFY playback_changed),
    current_cover: qt_property!(QString; NOTIFY playback_changed),
    current_audio: qt_property!(QVariantMap; NOTIFY track_details_changed),
    track_details_changed: qt_signal!(),
    lyrics: qt_property!(QVariantList; NOTIFY lyrics_changed),
    lyrics_synchronized: qt_property!(bool; NOTIFY lyrics_changed),
    lyrics_loading: qt_property!(bool; NOTIFY lyrics_changed),
    lyrics_error: qt_property!(QString; NOTIFY lyrics_changed),
    lyrics_changed: qt_signal!(),
    lyrics_visible: bool,
    capsule_active: bool,
    lyrics_worker_running: bool,
    lyrics_generation: Option<LoadGeneration>,
    set_lyrics_visible: qt_method!(
        fn set_lyrics_visible(&mut self, visible: bool) {
            self.lyrics_visible = visible;
            if visible {
                self.load_lyrics();
            }
        }
    ),
    set_capsule_active: qt_method!(
        fn set_capsule_active(&mut self, active: bool) {
            self.capsule_active = active;
            if active {
                self.load_lyrics();
            }
        }
    ),
    playback_error: qt_property!(QString; NOTIFY playback_changed),
    playback_changed: qt_signal!(),
    playback_position: qt_property!(i64; NOTIFY playback_position_changed),
    playback_position_changed: qt_signal!(),
    playback_duration: qt_property!(i64; NOTIFY playback_duration_changed),
    playback_duration_changed: qt_signal!(),
    player_volume: qt_property!(f64; NOTIFY player_volume_changed),
    player_volume_changed: qt_signal!(),
    play_mode: qt_property!(QString; NOTIFY play_mode_changed),
    play_mode_changed: qt_signal!(),
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
    cycle_play_mode: qt_method!(
        fn cycle_play_mode(&mut self) {
            self.cycle_play_mode_internal();
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
    play_queue_row: qt_method!(
        fn play_queue_row(&mut self, row: i32) {
            self.play_queue_row_internal(row);
        }
    ),
    remove_queue_row: qt_method!(
        fn remove_queue_row(&mut self, row: i32) {
            self.remove_queue_row_internal(row);
        }
    ),
    move_queue_row: qt_method!(
        fn move_queue_row(&mut self, from: i32, to: i32) {
            self.move_queue_row_internal(from, to);
        }
    ),
}

impl PlaybackController {
    pub fn set_volume_persist(&mut self, persist: impl Fn(f64) + Send + Sync + 'static) {
        self.persist_volume = Some(Arc::new(persist));
    }

    pub fn set_play_mode_persist(&mut self, persist: impl Fn(PlayMode) + Send + Sync + 'static) {
        self.persist_play_mode = Some(Arc::new(persist));
    }

    #[must_use]
    pub fn volume(&self) -> f64 {
        self.engine.volume
    }

    #[must_use]
    pub fn play_mode(&self) -> PlayMode {
        self.engine.mode
    }

    pub fn apply_saved_volume(&mut self, volume: f64) {
        self.engine.volume = volume.clamp(0.0, 1.0);
        self.player_volume = self.engine.volume;
        self.player_volume_changed();
        if let Some(player) = self.player.as_mut() {
            let _ = player.set_volume(self.engine.volume);
        }
    }

    pub fn apply_saved_play_mode(&mut self, mode: PlayMode) {
        self.engine.set_mode(mode);
        self.sync_queue_model();
        self.publish_identity();
        self.publish_play_mode();
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
            self.engine.prepare_queue(index);
            self.play_queue_index(index);
        } else {
            self.sync_queue_model();
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
        self.engine.load_failed = false;
        self.engine.note_pending(load_id, CommandKind::Load);
        self.engine.generation = player.generation();
        self.engine.current = Some(index);
        self.engine.sync_shuffle_index(index);
        self.engine.requested = PlaybackState::Playing;
        self.engine.publish_current();
        let play_id = player.play();
        self.engine.note_pending(play_id, CommandKind::Play);
        self.publish_track_details();
        self.sync_queue_model();
        self.publish_identity();
        self.publish_position();
        self.publish_duration();
    }

    fn publish_track_details(&mut self) {
        let track = &self.engine.queue[self.engine.current.unwrap()].metadata;
        let audio = &track.audio;
        let mut fields = QVariantMap::default();
        fields.insert(
            "format".into(),
            QString::from(audio.format.as_deref().unwrap_or_default()).into(),
        );
        fields.insert(
            "sampleRate".into(),
            audio.sample_rate.unwrap_or_default().into(),
        );
        fields.insert(
            "bitDepth".into(),
            u32::from(audio.bit_depth.unwrap_or_default()).into(),
        );
        fields.insert(
            "channels".into(),
            u32::from(audio.channels.unwrap_or_default()).into(),
        );
        fields.insert("bitrate".into(), audio.bitrate.unwrap_or_default().into());
        fields.insert("fileSize".into(), (track.file_size as f64).into());
        fields.insert(
            "path".into(),
            QString::from(track.path.to_string_lossy().as_ref()).into(),
        );
        self.current_audio = fields;
        self.track_details_changed();
        self.lyrics = QVariantList::default();
        self.lyrics_generation = None;
        self.lyrics_synchronized = false;
        self.lyrics_error = QString::default();
        self.lyrics_loading = self.lyrics_visible || self.capsule_active;
        self.lyrics_changed();
        if self.lyrics_visible || self.capsule_active {
            self.load_lyrics();
        }
    }

    fn load_lyrics(&mut self) {
        if self.lyrics_worker_running || self.lyrics_generation == Some(self.engine.generation) {
            return;
        }
        let Some(track) = self.engine.current.and_then(|i| self.engine.queue.get(i)) else {
            return;
        };
        let path = track.metadata.path.clone();
        let generation = self.engine.generation;
        let controller = QPointer::from(&*self);
        let deliver = qmetaobject::queued_callback(
            move |result: Result<qingyin_metadata::lyrics::Lyrics, String>| {
                let Some(controller) = controller.as_pinned() else {
                    return;
                };
                let mut controller = controller.borrow_mut();
                controller.finish_lyrics(generation, result);
            },
        );
        self.lyrics_worker_running = true;
        self.lyrics_loading = true;
        self.lyrics_changed();
        if let Err(error) = std::thread::Builder::new()
            .name("qingyin-lyrics".into())
            .spawn(move || {
                deliver(qingyin_metadata::lyrics::read(&path));
            })
        {
            self.lyrics_worker_running = false;
            self.lyrics_loading = false;
            self.lyrics_error = error.to_string().into();
            self.lyrics_changed();
        }
    }

    fn finish_lyrics(
        &mut self,
        generation: LoadGeneration,
        result: Result<qingyin_metadata::lyrics::Lyrics, String>,
    ) {
        self.lyrics_worker_running = false;
        if generation != self.engine.generation {
            if self.lyrics_visible || self.capsule_active {
                self.load_lyrics();
            }
            return;
        }
        self.lyrics_loading = false;
        self.lyrics_generation = Some(generation);
        match result {
            Ok(lyrics) => {
                self.lyrics_synchronized = lyrics.synchronized;
                self.lyrics = lyrics
                    .lines
                    .into_iter()
                    .map(|line| {
                        let mut item = QVariantMap::default();
                        item.insert("text".into(), QString::from(line.text).into());
                        item.insert("time".into(), line.time_ms.unwrap_or(-1).into());
                        QVariant::from(item)
                    })
                    .collect();
            }
            Err(error) => self.lyrics_error = error.into(),
        }
        self.lyrics_changed();
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
        self.engine.note_pending(
            id,
            if self.engine.requested == PlaybackState::Paused {
                CommandKind::Pause
            } else {
                CommandKind::Play
            },
        );
        self.publish_identity();
    }

    fn play_next_internal(&mut self) {
        match self.engine.next_index() {
            Some(index) => self.play_queue_index(index),
            None => self.stop_playback_silently(),
        }
    }

    fn cycle_play_mode_internal(&mut self) {
        let mode = self.engine.mode.cycled();
        self.engine.set_mode(mode);
        self.sync_queue_model();
        self.publish_identity();
        self.publish_play_mode();
        if let Some(persist) = &self.persist_play_mode {
            persist(mode);
        }
    }

    fn play_eos_internal(&mut self) {
        match self.engine.eos_index() {
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
        self.engine.note_pending(id, CommandKind::Seek);
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
        if self.engine.take_stop_after_load_failure()
            && let Some(player) = self.player.as_mut()
        {
            let id = player.stop();
            self.engine.note_pending(id, CommandKind::Stop);
        }
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
            self.play_eos_internal();
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
        self.current_queue_index = self
            .engine
            .current_playback_row()
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1);
        self.current_track_id = self
            .engine
            .current
            .and_then(|index| self.engine.queue.get(index))
            .map_or(0, TrackSnapshot::id);
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

    fn publish_play_mode(&mut self) {
        self.play_mode = self.engine.mode.as_str().into();
        self.play_mode_changed();
    }

    fn sync_queue_model(&mut self) {
        let order = self.engine.playback_order();
        let tracks = order
            .iter()
            .filter_map(|index| self.engine.queue.get(*index).cloned())
            .collect();
        self.queue_model.borrow_mut().reset(tracks);
    }

    fn play_queue_row_internal(&mut self, row: i32) {
        let Ok(display) = usize::try_from(row) else {
            return;
        };
        let Some(index) = self.engine.queue_index_at_playback_row(display) else {
            return;
        };
        self.engine.sync_shuffle_index(index);
        self.play_queue_index(index);
    }

    fn remove_queue_row_internal(&mut self, row: i32) {
        let Ok(display) = usize::try_from(row) else {
            return;
        };
        let Some(removed_current) = self.engine.remove_playback_row(display) else {
            return;
        };
        if removed_current {
            if let Some(next) = self.engine.current {
                self.play_queue_index(next);
            } else {
                self.sync_queue_model();
                self.stop_playback_silently();
            }
        } else {
            self.sync_queue_model();
            self.publish_identity();
        }
    }

    fn move_queue_row_internal(&mut self, from: i32, to: i32) {
        let Ok(from_index) = usize::try_from(from) else {
            return;
        };
        let Ok(to_index) = usize::try_from(to) else {
            return;
        };
        if !self.engine.move_playback_row(from_index, to_index) {
            return;
        }
        self.sync_queue_model();
        self.publish_identity();
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
    use super::{shuffle_order_avoiding, shuffle_order_from, *};
    use qingyin_core::PlayMode;
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
    fn stale_lyrics_cannot_replace_the_current_track() {
        let mut controller = PlaybackController::default();
        controller.engine.generation = 2;
        controller.finish_lyrics(1, Ok(qingyin_metadata::lyrics::parse("[00:01]old")));
        assert!(controller.lyrics.is_empty());
        assert_eq!(controller.lyrics_generation, None);
        controller.finish_lyrics(2, Ok(qingyin_metadata::lyrics::parse("[00:01]current")));
        assert_eq!(controller.lyrics.len(), 1);
        assert!(controller.lyrics_synchronized);
        assert_eq!(controller.lyrics_generation, Some(2));
        controller.finish_lyrics(1, Err("stale error".into()));
        assert!(controller.lyrics_error.to_string().is_empty());
        assert_eq!(controller.lyrics.len(), 1);
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
    fn load_failure_survives_the_following_play_command() {
        let mut engine = PlaybackEngine::default();
        engine.generation = 5;
        engine.note_pending(1, CommandKind::Load);
        engine.note_pending(2, CommandKind::Play);
        engine.handle_event(PlayerEvent::CommandFinished {
            id: 1,
            kind: CommandKind::Load,
            error: Some("missing file".into()),
        });
        assert_eq!(engine.confirmed, PlaybackState::Stopped);
        assert_eq!(engine.error, "missing file");
        assert!(engine.take_stop_after_load_failure());
        engine.handle_event(PlayerEvent::CommandFinished {
            id: 2,
            kind: CommandKind::Play,
            error: None,
        });
        engine.handle_event(PlayerEvent::StateChanged {
            generation: 5,
            state: PlaybackState::Playing,
        });
        assert_eq!(engine.confirmed, PlaybackState::Stopped);
        assert_eq!(engine.error, "missing file");
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
    fn remove_keeps_the_current_track_and_shuffle_sequence() {
        let mut engine = queued_engine(4);
        engine.mode = PlayMode::Shuffle;
        engine.current = Some(1);
        engine.shuffle_order = vec![1, 3, 0, 2];
        engine.shuffle_index = 0;
        assert_eq!(engine.remove_at(0), Some(false));
        assert_eq!(engine.current, Some(0));
        assert_eq!(engine.queue[0].metadata.title, "t1");
        assert_eq!(
            shuffle_titles(&engine),
            vec!["t1".to_string(), "t3".to_string(), "t2".to_string()]
        );
    }

    #[test]
    fn removing_the_current_track_selects_the_next_row() {
        let mut engine = queued_engine(3);
        engine.current = Some(1);
        assert_eq!(engine.remove_at(1), Some(true));
        assert_eq!(engine.current, Some(1));
        assert_eq!(engine.queue[1].metadata.title, "t2");
        assert_eq!(engine.remove_at(1), Some(true));
        assert_eq!(engine.current, Some(0));
        assert_eq!(engine.queue[0].metadata.title, "t0");
    }

    #[test]
    fn shuffle_list_order_is_the_order_that_plays_next() {
        let mut engine = queued_engine(4);
        engine.mode = PlayMode::Shuffle;
        engine.current = Some(1);
        engine.shuffle_order = vec![1, 3, 0, 2];
        engine.shuffle_index = 0;
        assert_eq!(engine.playback_order(), vec![1, 3, 0, 2]);
        assert_eq!(engine.current_playback_row(), Some(0));
        assert_eq!(engine.next_index(), Some(3));
        assert!(engine.move_playback_row(1, 3));
        assert_eq!(engine.playback_order(), vec![1, 0, 2, 3]);
        assert_eq!(engine.next_index(), Some(0));
    }

    #[test]
    fn move_changes_display_order_without_changing_shuffle_tracks() {
        let mut engine = queued_engine(4);
        engine.mode = PlayMode::Shuffle;
        engine.current = Some(0);
        engine.shuffle_order = vec![0, 2, 1, 3];
        engine.shuffle_index = 0;
        let before = shuffle_titles(&engine);
        assert!(engine.move_item(0, 2));
        assert_eq!(engine.current, Some(2));
        assert_eq!(
            engine
                .queue
                .iter()
                .map(|track| track.metadata.title.as_str())
                .collect::<Vec<_>>(),
            ["t1", "t2", "t0", "t3"]
        );
        assert_eq!(shuffle_titles(&engine), before);
    }

    fn shuffle_titles(engine: &PlaybackEngine) -> Vec<String> {
        engine
            .shuffle_order
            .iter()
            .map(|index| engine.queue[*index].metadata.title.clone())
            .collect()
    }

    #[test]
    fn sequential_navigation_wraps_the_queue() {
        let mut engine = queued_engine(3);
        engine.current = Some(2);
        assert_eq!(engine.next_index(), Some(0));
        engine.current = Some(0);
        assert_eq!(engine.previous_index(), Some(2));
        assert_eq!(engine.eos_index(), Some(1));
    }

    #[test]
    fn shuffle_walks_a_permutation_and_previous_is_stable() {
        let mut engine = queued_engine(4);
        engine.mode = PlayMode::Shuffle;
        engine.current = Some(1);
        engine.shuffle_order = vec![1, 0, 3, 2];
        engine.shuffle_index = 0;
        let next = engine.next_index().unwrap();
        assert_eq!(next, 0);
        engine.current = Some(next);
        assert_eq!(engine.previous_index(), Some(1));
        let mut seen = vec![1];
        engine.current = Some(1);
        engine.shuffle_index = 0;
        for _ in 0..3 {
            let index = engine.next_index().unwrap();
            engine.current = Some(index);
            seen.push(index);
        }
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 4);
    }

    #[test]
    fn shuffle_wrap_does_not_repeat_the_last_track() {
        let mut engine = queued_engine(5);
        engine.mode = PlayMode::Shuffle;
        engine.current = Some(4);
        engine.shuffle_order = vec![0, 1, 2, 3, 4];
        engine.shuffle_index = 4;
        let next = engine.next_index().unwrap();
        assert_ne!(next, 4);
        assert_eq!(engine.shuffle_index, 0);
        let mut sorted = engine.shuffle_order.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn repeat_one_replays_current_on_eos_but_next_still_moves() {
        let mut engine = queued_engine(3);
        engine.mode = PlayMode::RepeatOne;
        engine.current = Some(1);
        assert_eq!(engine.eos_index(), Some(1));
        assert_eq!(engine.next_index(), Some(2));
        engine.current = Some(2);
        assert_eq!(engine.next_index(), Some(0));
    }

    #[test]
    fn shuffle_helpers_build_unique_orders() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let mut rng = StdRng::seed_from_u64(11);
        let order = shuffle_order_from(8, 3, &mut rng);
        assert_eq!(order[0], 3);
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, (0..8).collect::<Vec<_>>());

        for seed in 0..32_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let order = shuffle_order_avoiding(6, 4, &mut rng);
            assert_ne!(order[0], 4);
            let mut sorted = order.clone();
            sorted.sort();
            assert_eq!(sorted, (0..6).collect::<Vec<_>>());
        }
    }

    fn queued_engine(count: usize) -> PlaybackEngine {
        let mut engine = PlaybackEngine::default();
        engine.queue = (0..count)
            .map(|index| snapshot(&format!("t{index}"), &format!("/tmp/{index}.flac")))
            .collect();
        engine
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
