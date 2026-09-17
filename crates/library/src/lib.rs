use qingyin_chinese::{SearchKey, search_key};
use qingyin_metadata::TrackMetadata;

#[derive(Debug, Default)]
pub struct MusicLibrary {
    tracks: Vec<TrackMetadata>,
}

impl MusicLibrary {
    #[must_use]
    pub fn tracks(&self) -> &[TrackMetadata] {
        &self.tracks
    }

    pub fn replace_tracks(&mut self, tracks: Vec<TrackMetadata>) {
        self.tracks = tracks;
    }

    #[must_use]
    pub fn search_key_for(track: &TrackMetadata) -> SearchKey {
        search_key(&track.title)
    }
}
