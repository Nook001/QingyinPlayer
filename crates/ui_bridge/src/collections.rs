use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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
pub(crate) const TRACK_HI_RES_ROLE: i32 = TRACK_TITLE_ROLE + 7;

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
        (TRACK_HI_RES_ROLE, "hiRes".into()),
    ])
}

fn is_hi_res(audio: &qingyin_metadata::AudioProperties) -> bool {
    audio.bit_depth.unwrap_or(0) >= 24 || audio.sample_rate.unwrap_or(0) >= 88_200
}

pub(crate) type TrackCatalog = Rc<RefCell<HashMap<i64, TrackSnapshot>>>;

fn track_role_data(track: &TrackMetadata, cover: &str, role: i32) -> QVariant {
    match role {
        TRACK_TITLE_ROLE => QString::from(track.title.as_str()).into(),
        TRACK_ARTIST_ROLE => QString::from(track.artists.join("、")).into(),
        TRACK_ALBUM_ROLE => QString::from(track.album.as_deref().unwrap_or_default()).into(),
        TRACK_DURATION_ROLE => QString::from(crate::format_duration(track)).into(),
        TRACK_PATH_ROLE => QString::from(track.path.to_string_lossy().as_ref()).into(),
        TRACK_COVER_ROLE => QString::from(cover).into(),
        TRACK_ID_ROLE => track.id.into(),
        TRACK_HI_RES_ROLE => is_hi_res(&track.audio).into(),
        _ => QVariant::default(),
    }
}

#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct CollectionModel {
    base: qt_base_class!(trait QAbstractListModel),
    entries: Vec<CollectionEntry>,
    search_terms: Vec<String>,
    visible_rows: Vec<usize>,
    query: String,
}

impl CollectionModel {
    pub fn reset(&mut self, entries: Vec<CollectionEntry>) {
        self.begin_reset_model();
        self.search_terms = entries
            .iter()
            .map(|entry| {
                let mut text = entry.name.clone();
                if let Some(path) = entry.id.strip_prefix("directory:") {
                    text.push_str(path);
                } else if entry.id.starts_with("album:") {
                    for track in &entry.tracks {
                        for artist in track
                            .metadata
                            .album_artist
                            .iter()
                            .chain(&track.metadata.artists)
                        {
                            text.push(' ');
                            text.push_str(artist);
                        }
                    }
                }
                text.to_lowercase()
            })
            .collect();
        self.entries = entries;
        self.update_visible_rows();
        self.end_reset_model();
    }

    pub fn set_filter(&mut self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if self.query == query {
            return false;
        }
        self.begin_reset_model();
        self.query = query;
        self.update_visible_rows();
        self.end_reset_model();
        true
    }

    fn update_visible_rows(&mut self) {
        self.visible_rows = self
            .search_terms
            .iter()
            .enumerate()
            .filter_map(|(row, text)| text.contains(&self.query).then_some(row))
            .collect();
    }

    #[must_use]
    pub fn entry_by_id(&self, id: &str) -> Option<CollectionEntry> {
        self.entries.iter().find(|entry| entry.id == id).cloned()
    }
}

/// Visible library or search rows. Row order is track ids; tags live in `catalog`.
#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct TrackListModel {
    base: qt_base_class!(trait QAbstractListModel),
    catalog: TrackCatalog,
    rows: Vec<i64>,
    index_of_track: qt_method!(
        fn index_of_track(&self, track_id: i64) -> i32 {
            row_index_of(&self.rows, track_id)
        }
    ),
    track_id_at: qt_method!(
        fn track_id_at(&self, row: i32) -> i64 {
            track_id_at(&self.rows, row)
        }
    ),
}

impl TrackListModel {
    pub fn bind_catalog(&mut self, catalog: TrackCatalog) {
        self.catalog = catalog;
    }

    /// Test helper: keeps these snapshots in the model's own catalog.
    #[cfg(test)]
    pub fn replace(&mut self, tracks: Vec<TrackSnapshot>) {
        let rows = intern_tracks(&self.catalog, tracks);
        self.set_rows(rows);
    }

    pub fn set_rows(&mut self, rows: Vec<i64>) {
        self.begin_reset_model();
        self.rows = rows;
        self.end_reset_model();
    }

    pub fn sort_in_place(&mut self, column: SortColumn, ascending: bool) {
        if self.rows.is_empty() {
            return;
        }
        {
            let catalog = self.catalog.borrow();
            sort_row_ids(&mut self.rows, &catalog, column, ascending);
        }
        self.notify_all_rows();
    }

    pub fn upsert_id(&mut self, id: i64) {
        if let Some(index) = self.rows.iter().position(|row| *row == id) {
            self.notify_row(index);
            return;
        }
        let Ok(row) = i32::try_from(self.rows.len()) else {
            return;
        };
        self.begin_insert_rows(row, row);
        self.rows.push(id);
        self.end_insert_rows();
    }

    pub fn remove_ids(&mut self, ids: &[i64]) {
        for id in ids {
            if let Some(index) = self.rows.iter().position(|row| row == id) {
                let Ok(row) = i32::try_from(index) else {
                    continue;
                };
                self.begin_remove_rows(row, row);
                self.rows.remove(index);
                self.end_remove_rows();
            }
        }
    }

    pub fn notify_track(&mut self, track_id: i64) {
        if let Some(index) = self.rows.iter().position(|row| *row == track_id) {
            self.notify_row(index);
        }
    }

    fn notify_row(&mut self, index: usize) {
        if let Ok(row) = i32::try_from(index) {
            let index = self.row_index(row);
            self.data_changed(index, index);
        }
    }

    fn notify_all_rows(&mut self) {
        if let Ok(last) = i32::try_from(self.rows.len().saturating_sub(1)) {
            let top = self.row_index(0);
            let bottom = self.row_index(last);
            self.data_changed(top, bottom);
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<TrackSnapshot> {
        snapshot_rows(&self.catalog, &self.rows)
    }

    #[cfg(test)]
    #[must_use]
    pub fn position_of_id(&self, id: i64) -> Option<usize> {
        self.rows.iter().position(|row| *row == id)
    }
}

#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct DetailTrackModel {
    base: qt_base_class!(trait QAbstractListModel),
    catalog: TrackCatalog,
    rows: Vec<i64>,
    index_of_track: qt_method!(
        fn index_of_track(&self, track_id: i64) -> i32 {
            row_index_of(&self.rows, track_id)
        }
    ),
    track_id_at: qt_method!(
        fn track_id_at(&self, row: i32) -> i64 {
            track_id_at(&self.rows, row)
        }
    ),
}

impl DetailTrackModel {
    pub fn bind_catalog(&mut self, catalog: TrackCatalog) {
        self.catalog = catalog;
    }

    pub fn set_rows(&mut self, rows: Vec<i64>) {
        self.begin_reset_model();
        self.rows = rows;
        self.end_reset_model();
    }

    pub fn notify_track(&mut self, track_id: i64) {
        if let Some(index) = self.rows.iter().position(|row| *row == track_id)
            && let Ok(row) = i32::try_from(index)
        {
            let index = self.row_index(row);
            self.data_changed(index, index);
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<TrackSnapshot> {
        snapshot_rows(&self.catalog, &self.rows)
    }
}

impl QAbstractListModel for TrackListModel {
    fn row_count(&self) -> i32 {
        i32::try_from(self.rows.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Ok(row) = usize::try_from(index.row()) else {
            return QVariant::default();
        };
        let Some(id) = self.rows.get(row).copied() else {
            return QVariant::default();
        };
        let catalog = self.catalog.borrow();
        let Some(track) = catalog.get(&id) else {
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
        i32::try_from(self.rows.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Ok(row) = usize::try_from(index.row()) else {
            return QVariant::default();
        };
        let Some(id) = self.rows.get(row).copied() else {
            return QVariant::default();
        };
        let catalog = self.catalog.borrow();
        let Some(track) = catalog.get(&id) else {
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
        i32::try_from(self.visible_rows.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(entry) = usize::try_from(index.row())
            .ok()
            .and_then(|row| self.visible_rows.get(row))
            .and_then(|&row| self.entries.get(row))
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

fn row_index_of(rows: &[i64], track_id: i64) -> i32 {
    rows.iter()
        .position(|row| *row == track_id)
        .and_then(|row| i32::try_from(row).ok())
        .unwrap_or(-1)
}

fn track_id_at(rows: &[i64], row: i32) -> i64 {
    usize::try_from(row)
        .ok()
        .and_then(|row| rows.get(row).copied())
        .unwrap_or(0)
}

#[cfg(test)]
fn intern_tracks(catalog: &TrackCatalog, tracks: Vec<TrackSnapshot>) -> Vec<i64> {
    let mut catalog = catalog.borrow_mut();
    let mut rows = Vec::with_capacity(tracks.len());
    for track in tracks {
        let id = track.id();
        catalog.insert(id, track);
        rows.push(id);
    }
    rows
}

fn snapshot_rows(catalog: &TrackCatalog, rows: &[i64]) -> Vec<TrackSnapshot> {
    let catalog = catalog.borrow();
    rows.iter()
        .filter_map(|id| catalog.get(id).cloned())
        .collect()
}

fn sort_row_ids(
    rows: &mut [i64],
    catalog: &HashMap<i64, TrackSnapshot>,
    column: SortColumn,
    ascending: bool,
) {
    rows.sort_by(|left_id, right_id| {
        let Some(left) = catalog.get(left_id) else {
            return std::cmp::Ordering::Equal;
        };
        let Some(right) = catalog.get(right_id) else {
            return std::cmp::Ordering::Equal;
        };
        let ordering = left.cmp_column(right, column);
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn collection_filters_use_their_scope_and_survive_refresh() {
        let mut metadata = TrackMetadata::from_display(
            "/music/Live/song.flac",
            "Song",
            Some("夏天".into()),
            vec!["乐队".into()],
            None,
        );
        metadata.album_artist = Some("Various Artists".into());
        let tracks = vec![TrackSnapshot::from_metadata(metadata)];
        let mut model = CollectionModel::default();
        model.reset(qingyin_library::aggregate_albums(&tracks));
        assert!(model.set_filter("VARIOUS"));
        assert_eq!(model.row_count(), 1);
        model.set_filter("不存在");
        assert_eq!(model.row_count(), 0);
        model.reset(qingyin_library::aggregate_albums(&tracks));
        assert_eq!(model.row_count(), 0);
        model.set_filter("  夏天  ");
        assert_eq!(model.row_count(), 1);
        assert_eq!(
            model
                .entry_by_id("album:夏天\u{1f}various artists")
                .unwrap()
                .track_count,
            1
        );

        model.reset(qingyin_library::aggregate_directories(
            &tracks,
            &[std::path::PathBuf::from("/music")],
        ));
        model.set_filter("/music");
        assert_eq!(model.row_count(), 1);
        model.set_filter("歌曲");
        assert_eq!(model.row_count(), 0);
        model.set_filter("");
        assert_eq!(model.row_count(), 1);

        model.reset(qingyin_library::aggregate_artists(&tracks));
        model.set_filter("夏天");
        assert_eq!(model.row_count(), 0);
        model.set_filter("乐队");
        assert_eq!(model.row_count(), 1);
    }

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
        assert_eq!(
            track_role_data(&track, "", TRACK_HI_RES_ROLE),
            QVariant::from(false)
        );
        let mut hires = track.clone();
        hires.audio.bit_depth = Some(24);
        assert_eq!(
            track_role_data(&hires, "", TRACK_HI_RES_ROLE),
            QVariant::from(true)
        );
        hires.audio.bit_depth = None;
        hires.audio.sample_rate = Some(88_200);
        assert_eq!(
            track_role_data(&hires, "", TRACK_HI_RES_ROLE),
            QVariant::from(true)
        );
        hires.audio.sample_rate = Some(44_100);
        assert_eq!(
            track_role_data(&hires, "", TRACK_HI_RES_ROLE),
            QVariant::from(false)
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
