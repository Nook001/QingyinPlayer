use std::collections::VecDeque;
use std::path::PathBuf;

use qingyin_library::MusicLibrary;
use qingyin_metadata::TrackMetadata;
use qingyin_player::PlaybackState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub music_directories: Vec<PathBuf>,
}

#[derive(Debug, Default)]
pub struct PlaybackQueue {
    tracks: VecDeque<TrackMetadata>,
}

impl PlaybackQueue {
    pub fn push(&mut self, track: TrackMetadata) {
        self.tracks.push_back(track);
    }

    pub fn pop(&mut self) -> Option<TrackMetadata> {
        self.tracks.pop_front()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct AppCore {
    pub library: MusicLibrary,
    pub queue: PlaybackQueue,
    pub playback_state: PlaybackState,
    pub settings: Settings,
}
