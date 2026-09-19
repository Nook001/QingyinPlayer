use std::collections::HashMap;

use qingyin_library::CollectionEntry;
use qingyin_metadata::TrackMetadata;
use qmetaobject::prelude::*;

const NAME_ROLE: i32 = 0x0100;
const SUBTITLE_ROLE: i32 = NAME_ROLE + 1;
const COVER_ROLE: i32 = NAME_ROLE + 2;
const TRACK_COUNT_ROLE: i32 = NAME_ROLE + 3;
const TITLE_ROLE: i32 = 0x0100;
const ARTIST_ROLE: i32 = TITLE_ROLE + 1;
const ALBUM_ROLE: i32 = TITLE_ROLE + 2;
const DURATION_ROLE: i32 = TITLE_ROLE + 3;
const PATH_ROLE: i32 = TITLE_ROLE + 4;
const TRACK_COVER_ROLE: i32 = TITLE_ROLE + 5;

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

#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct DetailTrackModel {
    base: qt_base_class!(trait QAbstractListModel),
    tracks: Vec<TrackMetadata>,
    cover_urls: Vec<String>,
}

impl DetailTrackModel {
    pub fn reset(&mut self, tracks: Vec<TrackMetadata>, cover_urls: Vec<String>) {
        self.begin_reset_model();
        self.tracks = tracks;
        self.cover_urls = cover_urls;
        self.cover_urls.resize(self.tracks.len(), String::new());
        self.end_reset_model();
    }

    #[must_use]
    pub fn snapshot(&self) -> (Vec<TrackMetadata>, Vec<String>) {
        (self.tracks.clone(), self.cover_urls.clone())
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

        match role {
            TITLE_ROLE => QString::from(track.title.clone()).into(),
            ARTIST_ROLE => QString::from(track.artists.join("、")).into(),
            ALBUM_ROLE => QString::from(track.album.clone().unwrap_or_default()).into(),
            DURATION_ROLE => QString::from(format_duration(track)).into(),
            PATH_ROLE => QString::from(track.path.to_string_lossy().into_owned()).into(),
            TRACK_COVER_ROLE => {
                QString::from(self.cover_urls.get(row).cloned().unwrap_or_default()).into()
            }
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        HashMap::from([
            (TITLE_ROLE, "title".into()),
            (ARTIST_ROLE, "artist".into()),
            (ALBUM_ROLE, "album".into()),
            (DURATION_ROLE, "duration".into()),
            (PATH_ROLE, "path".into()),
            (TRACK_COVER_ROLE, "cover".into()),
        ])
    }
}

fn format_duration(track: &TrackMetadata) -> String {
    let seconds = track.duration.map_or(0, |duration| duration.as_secs());
    format!("{}:{:02}", seconds / 60, seconds % 60)
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
            NAME_ROLE => QString::from(entry.name.clone()).into(),
            SUBTITLE_ROLE => QString::from(entry.subtitle.clone()).into(),
            COVER_ROLE => QString::from(entry.cover_url.clone()).into(),
            TRACK_COUNT_ROLE => i64::try_from(entry.track_count).unwrap_or(i64::MAX).into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        HashMap::from([
            (NAME_ROLE, "name".into()),
            (SUBTITLE_ROLE, "subtitle".into()),
            (COVER_ROLE, "cover".into()),
            (TRACK_COUNT_ROLE, "track_count".into()),
        ])
    }
}
