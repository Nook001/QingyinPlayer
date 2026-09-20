use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use qingyin_library::CollectionEntry;
use qingyin_metadata::TrackMetadata;
use qmetaobject::prelude::*;

pub(crate) const TRACK_TITLE_ROLE: i32 = 0x0100;
pub(crate) const TRACK_ARTIST_ROLE: i32 = TRACK_TITLE_ROLE + 1;
pub(crate) const TRACK_ALBUM_ROLE: i32 = TRACK_TITLE_ROLE + 2;
pub(crate) const TRACK_DURATION_ROLE: i32 = TRACK_TITLE_ROLE + 3;
pub(crate) const TRACK_PATH_ROLE: i32 = TRACK_TITLE_ROLE + 4;
pub(crate) const TRACK_COVER_ROLE: i32 = TRACK_TITLE_ROLE + 5;

const COLLECTION_NAME_ROLE: i32 = 0x0100;
const COLLECTION_SUBTITLE_ROLE: i32 = COLLECTION_NAME_ROLE + 1;
const COLLECTION_COVER_ROLE: i32 = COLLECTION_NAME_ROLE + 2;
const COLLECTION_TRACK_COUNT_ROLE: i32 = COLLECTION_NAME_ROLE + 3;

fn track_role_names() -> HashMap<i32, QByteArray> {
    HashMap::from([
        (TRACK_TITLE_ROLE, "title".into()),
        (TRACK_ARTIST_ROLE, "artist".into()),
        (TRACK_ALBUM_ROLE, "album".into()),
        (TRACK_DURATION_ROLE, "duration".into()),
        (TRACK_PATH_ROLE, "path".into()),
        (TRACK_COVER_ROLE, "cover".into()),
    ])
}

fn track_role_data(track: &TrackMetadata, cover: &str, role: i32) -> QVariant {
    match role {
        TRACK_TITLE_ROLE => QString::from(track.title.clone()).into(),
        TRACK_ARTIST_ROLE => QString::from(track.artists.join("、")).into(),
        TRACK_ALBUM_ROLE => QString::from(track.album.clone().unwrap_or_default()).into(),
        TRACK_DURATION_ROLE => QString::from(crate::format_duration(track)).into(),
        TRACK_PATH_ROLE => QString::from(track.path.to_string_lossy().into_owned()).into(),
        TRACK_COVER_ROLE => QString::from(cover).into(),
        _ => QVariant::default(),
    }
}

#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct CollectionModel {
    base: qt_base_class!(trait QAbstractListModel),
    entries: Vec<CollectionEntry>,
}

impl CollectionModel {
    pub fn reset(&mut self, entries: Vec<CollectionEntry>) {
        self.begin_reset_model();
        self.entries = entries;
        self.end_reset_model();
    }

    #[must_use]
    pub fn entry(&self, row: usize) -> Option<CollectionEntry> {
        self.entries.get(row).cloned()
    }

    #[must_use]
    pub fn entry_by_name(&self, name: &str) -> Option<CollectionEntry> {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .cloned()
    }
}

/// Visible library or search rows bound by the library `TrackTable`.
#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct TrackListModel {
    base: qt_base_class!(trait QAbstractListModel),
    tracks: Vec<TrackMetadata>,
    cover_urls: Vec<String>,
}

impl TrackListModel {
    pub fn replace(&mut self, tracks: Vec<TrackMetadata>, cover_urls: Vec<String>) {
        self.begin_reset_model();
        self.tracks = tracks;
        self.cover_urls = cover_urls;
        self.cover_urls.resize(self.tracks.len(), String::new());
        self.end_reset_model();
    }

    pub fn sort_in_place(&mut self, column: &str, ascending: bool) {
        if self.tracks.is_empty() {
            return;
        }
        crate::sort_tracks(&mut self.tracks, &mut self.cover_urls, column, ascending);
        let last = i32::try_from(self.tracks.len().saturating_sub(1)).unwrap_or(i32::MAX);
        let top_left = self.row_index(0);
        let bottom_right = self.row_index(last);
        self.data_changed(top_left, bottom_right);
    }

    #[must_use]
    pub fn snapshot(&self) -> (Vec<TrackMetadata>, Vec<String>) {
        (self.tracks.clone(), self.cover_urls.clone())
    }

    #[must_use]
    pub fn position_of_path(&self, path: &Path) -> Option<usize> {
        self.tracks.iter().position(|track| track.path == path)
    }
}

#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct DetailTrackModel {
    base: qt_base_class!(trait QAbstractListModel),
    tracks: Vec<Arc<TrackMetadata>>,
    cover_urls: Vec<String>,
}

impl DetailTrackModel {
    pub fn reset(&mut self, tracks: Vec<Arc<TrackMetadata>>, cover_urls: Vec<String>) {
        self.begin_reset_model();
        self.tracks = tracks;
        self.cover_urls = cover_urls;
        self.cover_urls.resize(self.tracks.len(), String::new());
        self.end_reset_model();
    }

    #[must_use]
    pub fn snapshot(&self) -> (Vec<TrackMetadata>, Vec<String>) {
        (
            self.tracks
                .iter()
                .map(|track| track.as_ref().clone())
                .collect(),
            self.cover_urls.clone(),
        )
    }
}

impl QAbstractListModel for TrackListModel {
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
        let cover = self.cover_urls.get(row).map_or("", String::as_str);
        track_role_data(track, cover, role)
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        track_role_names()
    }
}

impl QAbstractListModel for DetailTrackModel {
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
        let cover = self.cover_urls.get(row).map_or("", String::as_str);
        track_role_data(track, cover, role)
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        track_role_names()
    }
}

impl QAbstractListModel for CollectionModel {
    fn row_count(&self) -> i32 {
        i32::try_from(self.entries.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(entry) = usize::try_from(index.row())
            .ok()
            .and_then(|row| self.entries.get(row))
        else {
            return QVariant::default();
        };

        match role {
            COLLECTION_NAME_ROLE => QString::from(entry.name.clone()).into(),
            COLLECTION_SUBTITLE_ROLE => QString::from(entry.subtitle.clone()).into(),
            COLLECTION_COVER_ROLE => QString::from(entry.cover_url.clone()).into(),
            COLLECTION_TRACK_COUNT_ROLE => {
                i64::try_from(entry.track_count).unwrap_or(i64::MAX).into()
            }
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        HashMap::from([
            (COLLECTION_NAME_ROLE, "name".into()),
            (COLLECTION_SUBTITLE_ROLE, "subtitle".into()),
            (COLLECTION_COVER_ROLE, "cover".into()),
            (COLLECTION_TRACK_COUNT_ROLE, "track_count".into()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn library_and_detail_models_share_track_roles() {
        let library = TrackListModel::default().role_names();
        let detail = DetailTrackModel::default().role_names();
        assert_eq!(library.len(), detail.len());
        for (role, name) in &library {
            assert_eq!(detail.get(role), Some(name));
        }

        let track = TrackMetadata::from_display(
            "/music/a.flac",
            "标题",
            Some("专辑".into()),
            vec!["歌手".into()],
            Some(Duration::from_secs(125)),
        );
        let title = track_role_data(&track, "file:///cover.png", TRACK_TITLE_ROLE);
        let cover = track_role_data(&track, "file:///cover.png", TRACK_COVER_ROLE);
        assert_eq!(title.to_qstring().to_string(), "标题");
        assert_eq!(cover.to_qstring().to_string(), "file:///cover.png");
        assert_eq!(
            track_role_data(&track, "", TRACK_DURATION_ROLE)
                .to_qstring()
                .to_string(),
            "2:05"
        );
    }
}
