use std::collections::HashMap;

use qingyin_core::SortColumn;
use qingyin_library::{CollectionEntry, TrackSnapshot};
use qingyin_metadata::TrackMetadata;
use qmetaobject::prelude::*;

pub(crate) const TRACK_TITLE_ROLE: i32 = 0x0100;
pub(crate) const TRACK_ARTIST_ROLE: i32 = TRACK_TITLE_ROLE + 1;
pub(crate) const TRACK_ALBUM_ROLE: i32 = TRACK_TITLE_ROLE + 2;
pub(crate) const TRACK_DURATION_ROLE: i32 = TRACK_TITLE_ROLE + 3;
pub(crate) const TRACK_PATH_ROLE: i32 = TRACK_TITLE_ROLE + 4;
pub(crate) const TRACK_COVER_ROLE: i32 = TRACK_TITLE_ROLE + 5;
pub(crate) const TRACK_ID_ROLE: i32 = TRACK_TITLE_ROLE + 6;

const COLLECTION_NAME_ROLE: i32 = 0x0100;
const COLLECTION_SUBTITLE_ROLE: i32 = COLLECTION_NAME_ROLE + 1;
const COLLECTION_COVER_ROLE: i32 = COLLECTION_NAME_ROLE + 2;
const COLLECTION_TRACK_COUNT_ROLE: i32 = COLLECTION_NAME_ROLE + 3;
const COLLECTION_ID_ROLE: i32 = COLLECTION_NAME_ROLE + 4;

fn track_role_names() -> HashMap<i32, QByteArray> {
    HashMap::from([
        (TRACK_TITLE_ROLE, "title".into()),
        (TRACK_ARTIST_ROLE, "artist".into()),
        (TRACK_ALBUM_ROLE, "album".into()),
        (TRACK_DURATION_ROLE, "duration".into()),
        (TRACK_PATH_ROLE, "path".into()),
        (TRACK_COVER_ROLE, "cover".into()),
        (TRACK_ID_ROLE, "trackId".into()),
    ])
}

fn track_role_data(track: &TrackMetadata, cover: &str, role: i32) -> QVariant {
    match role {
        TRACK_TITLE_ROLE => QString::from(track.title.as_str()).into(),
        TRACK_ARTIST_ROLE => QString::from(track.artists.join("、")).into(),
        TRACK_ALBUM_ROLE => QString::from(track.album.as_deref().unwrap_or_default()).into(),
        TRACK_DURATION_ROLE => QString::from(crate::format_duration(track)).into(),
        TRACK_PATH_ROLE => QString::from(track.path.to_string_lossy().as_ref()).into(),
        TRACK_COVER_ROLE => QString::from(cover).into(),
        TRACK_ID_ROLE => track.id.into(),
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
    pub fn entry_by_id(&self, id: &str) -> Option<CollectionEntry> {
        self.entries.iter().find(|entry| entry.id == id).cloned()
    }
}

/// Visible library or search rows bound by the library `TrackTable`.
#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct TrackListModel {
    base: qt_base_class!(trait QAbstractListModel),
    tracks: Vec<TrackSnapshot>,
}

impl TrackListModel {
    pub fn replace(&mut self, tracks: Vec<TrackSnapshot>) {
        self.begin_reset_model();
        self.tracks = tracks;
        self.end_reset_model();
    }

    pub fn sort_in_place(&mut self, column: SortColumn, ascending: bool) {
        if self.tracks.is_empty() {
            return;
        }
        crate::sort_snapshots(&mut self.tracks, column, ascending);
        // qmetaobject's list model does not expose layoutChanged; role data for every
        // row is refreshed so QML bindings keep the same TrackId on each snapshot.
        if let Ok(last) = i32::try_from(self.tracks.len().saturating_sub(1)) {
            let top = self.row_index(0);
            let bottom = self.row_index(last);
            self.data_changed(top, bottom);
        }
    }

    pub fn upsert(&mut self, snapshot: TrackSnapshot) {
        if let Some(index) = self
            .tracks
            .iter()
            .position(|track| track.id() == snapshot.id())
        {
            self.tracks[index] = snapshot;
            if let Ok(row) = i32::try_from(index) {
                let index = self.row_index(row);
                self.data_changed(index, index);
            }
            return;
        }
        let Ok(row) = i32::try_from(self.tracks.len()) else {
            return;
        };
        self.begin_insert_rows(row, row);
        self.tracks.push(snapshot);
        self.end_insert_rows();
    }

    pub fn remove_ids(&mut self, ids: &[i64]) {
        for id in ids {
            if let Some(index) = self.tracks.iter().position(|track| track.id() == *id) {
                let Ok(row) = i32::try_from(index) else {
                    continue;
                };
                self.begin_remove_rows(row, row);
                self.tracks.remove(index);
                self.end_remove_rows();
            }
        }
    }

    pub fn set_cover(&mut self, track_id: i64, url: String) {
        if let Some(index) = self.tracks.iter().position(|track| track.id() == track_id) {
            self.tracks[index].cover_url = url;
            if let Ok(row) = i32::try_from(index) {
                let index = self.row_index(row);
                self.data_changed(index, index);
            }
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<TrackSnapshot> {
        self.tracks.clone()
    }

    #[cfg(test)]
    #[must_use]
    pub fn position_of_id(&self, id: i64) -> Option<usize> {
        self.tracks.iter().position(|track| track.id() == id)
    }
}

#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct DetailTrackModel {
    base: qt_base_class!(trait QAbstractListModel),
    tracks: Vec<TrackSnapshot>,
}

impl DetailTrackModel {
    pub fn reset(&mut self, tracks: Vec<TrackSnapshot>) {
        self.begin_reset_model();
        self.tracks = tracks;
        self.end_reset_model();
    }

    pub fn set_cover(&mut self, track_id: i64, url: String) {
        if let Some(index) = self.tracks.iter().position(|track| track.id() == track_id) {
            self.tracks[index].cover_url = url;
            if let Ok(row) = i32::try_from(index) {
                let index = self.row_index(row);
                self.data_changed(index, index);
            }
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<TrackSnapshot> {
        self.tracks.clone()
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
        track_role_data(&track.metadata, &track.cover_url, role)
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
        track_role_data(&track.metadata, &track.cover_url, role)
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
            COLLECTION_ID_ROLE => QString::from(entry.id.clone()).into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        HashMap::from([
            (COLLECTION_NAME_ROLE, "name".into()),
            (COLLECTION_SUBTITLE_ROLE, "subtitle".into()),
            (COLLECTION_COVER_ROLE, "cover".into()),
            (COLLECTION_TRACK_COUNT_ROLE, "track_count".into()),
            (COLLECTION_ID_ROLE, "collectionId".into()),
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

    #[test]
    fn sort_keeps_stable_track_ids() {
        let mut first = TrackMetadata::from_display("/music/b.flac", "B", None, Vec::new(), None);
        first.id = 2;
        let mut second = TrackMetadata::from_display("/music/a.flac", "A", None, Vec::new(), None);
        second.id = 7;
        let mut model = TrackListModel::default();
        model.replace(vec![
            TrackSnapshot::from_metadata(first),
            TrackSnapshot::from_metadata(second),
        ]);
        model.sort_in_place(SortColumn::Title, true);
        assert_eq!(model.position_of_id(7), Some(0));
        assert_eq!(model.position_of_id(2), Some(1));
        assert_eq!(model.snapshot()[0].metadata.title, "A");
    }
}
