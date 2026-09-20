use std::path::Path;
use std::time::Duration;

use qingyin_chinese::{contains_han, search_key};
use qingyin_metadata::TrackMetadata;
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

pub type TrackId = i64;

const SEARCH_FIELD_TITLE: &str = "title";
const SEARCH_FIELD_ARTIST: &str = "artist";
const SEARCH_FIELD_ALBUM: &str = "album";
const SCHEMA_VERSION: u32 = 4;
const BUSY_TIMEOUT_MS: u32 = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTrack {
    pub id: TrackId,
    pub metadata: TrackMetadata,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("path is not valid UTF-8: {0}")]
    NonUtf8Path(String),
    #[error("track duration is too large to store")]
    DurationOverflow,
    #[error("track has too many artists to store")]
    ArtistPositionOverflow,
}

#[derive(Debug)]
pub struct Database {
    connection: Connection,
}

impl Database {
    /// Opens a database and applies pending schema migrations.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot open or migrate the database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        configure_connection(&connection)?;
        let database = Self { connection };
        database.migrate()?;
        Ok(database)
    }

    /// Opens an in-memory database and applies the initial schema.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot initialize the database.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        configure_connection(&connection)?;
        let database = Self { connection };
        database.migrate()?;
        Ok(database)
    }

    /// Inserts or replaces a track and its derived search data atomically.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when data conversion or an SQLite operation fails.
    pub fn upsert_track(
        &mut self,
        track: &TrackMetadata,
        modified_at: i64,
    ) -> Result<TrackId, StorageError> {
        let transaction = self.connection.transaction()?;
        let track_id = upsert_track_on(&transaction, track, modified_at)?;
        transaction.commit()?;
        Ok(track_id)
    }

    /// Inserts or replaces many tracks in a single transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when data conversion or an SQLite operation fails.
    pub fn upsert_tracks(&mut self, tracks: &[(&TrackMetadata, i64)]) -> Result<(), StorageError> {
        if tracks.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        for (track, modified_at) in tracks {
            upsert_track_on(&transaction, track, *modified_at)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Returns every stored track ordered by title and path.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot execute or decode the query.
    pub fn list_tracks(&self) -> Result<Vec<StoredTrack>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT tracks.id, tracks.path, tracks.title, tracks.album, tracks.duration_ms,
                    tracks.title_sort, tracks.album_sort, tracks.artist_sort, tracks.modified_at,
                    track_artists.artist
             FROM tracks
             LEFT JOIN track_artists ON track_artists.track_id = tracks.id
             ORDER BY tracks.title, tracks.path, track_artists.position",
        )?;
        collect_joined_tracks(statement.query_map([], joined_track_row)?)
    }

    /// Searches stored tracks by title, artist, or album.
    ///
    /// Queries containing Han characters match original field text only. Latin queries may
    /// match original text, full pinyin, or pinyin initials of a single field. Title matches
    /// are returned before artist and album matches.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot execute or decode the query.
    pub fn search_tracks(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<StoredTrack>, StorageError> {
        let query = search_key(query);
        if query.normalized.is_empty() {
            return Ok(Vec::new());
        }
        let han_query = i64::from(contains_han(&query.normalized));
        let mut statement = self.connection.prepare(
            "SELECT ranked.id, ranked.path, ranked.title, ranked.album, ranked.duration_ms,
                    ranked.title_sort, ranked.album_sort, ranked.artist_sort, ranked.modified_at,
                    track_artists.artist
             FROM (
                 SELECT tracks.id, tracks.path, tracks.title, tracks.album, tracks.duration_ms,
                        tracks.title_sort, tracks.album_sort, tracks.artist_sort,
                        tracks.modified_at,
                        MIN(CASE search_terms.field
                                WHEN 'title' THEN 0
                                WHEN 'artist' THEN 1
                                WHEN 'album' THEN 2
                                ELSE 3
                            END) AS match_rank
                 FROM tracks
                 JOIN search_terms ON search_terms.track_id = tracks.id
                 WHERE instr(search_terms.normalized, ?1) > 0
                    OR (?5 = 0 AND ?2 <> '' AND instr(search_terms.full_pinyin, ?2) > 0)
                    OR (?5 = 0 AND ?3 <> '' AND instr(search_terms.initials, ?3) > 0)
                 GROUP BY tracks.id
                 ORDER BY match_rank, tracks.title, tracks.path
                 LIMIT ?4
             ) AS ranked
             LEFT JOIN track_artists ON track_artists.track_id = ranked.id
             ORDER BY ranked.match_rank, ranked.title, ranked.path, track_artists.position",
        )?;
        collect_joined_tracks(statement.query_map(
            params![
                query.normalized,
                query.full_pinyin,
                query.initials,
                i64::try_from(limit).unwrap_or(i64::MAX),
                han_query
            ],
            joined_track_row,
        )?)
    }

    /// Removes a track by path and relies on foreign keys to remove related data.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the path is invalid or SQLite cannot delete the row.
    pub fn remove_track(&self, path: impl AsRef<Path>) -> Result<bool, StorageError> {
        let path = path_as_str(path.as_ref())?;
        Ok(self
            .connection
            .execute("DELETE FROM tracks WHERE path = ?1", [path])?
            > 0)
    }

    /// Removes every track stored at `directory` or nested under it.
    ///
    /// The root filesystem path is ignored so a bad event cannot wipe the library.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the path is invalid or SQLite cannot delete the rows.
    pub fn remove_tracks_under(&self, directory: impl AsRef<Path>) -> Result<usize, StorageError> {
        let directory = path_as_str(directory.as_ref())?;
        let directory = directory.trim_end_matches('/');
        if directory.is_empty() || directory == "/" {
            return Ok(0);
        }
        let pattern = format!("{}/%", escape_like_literal(directory));
        let deleted = self.connection.execute(
            "DELETE FROM tracks WHERE path = ?1 OR path LIKE ?2 ESCAPE '\\'",
            params![directory, pattern],
        )?;
        Ok(deleted)
    }

    /// Returns the modification time recorded for a path, if present.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the path is invalid or SQLite cannot execute the query.
    pub fn track_modified_at(&self, path: impl AsRef<Path>) -> Result<Option<i64>, StorageError> {
        let path = path_as_str(path.as_ref())?;
        self.connection
            .query_row(
                "SELECT modified_at FROM tracks WHERE path = ?1",
                [path],
                |row| row.get(0),
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn artists_for_track(&self, track_id: TrackId) -> Result<Vec<String>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT artist FROM track_artists WHERE track_id = ?1 ORDER BY position")?;
        statement
            .query_map([track_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()
            .map_err(StorageError::from)
    }

    fn migrate(&self) -> Result<(), StorageError> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS tracks (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL UNIQUE,
                 title TEXT NOT NULL,
                 album TEXT,
                 duration_ms INTEGER,
                 modified_at INTEGER NOT NULL,
                 title_sort TEXT,
                 album_sort TEXT,
                 artist_sort TEXT
             );
             CREATE TABLE IF NOT EXISTS track_artists (
                 track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                 artist TEXT NOT NULL,
                 position INTEGER NOT NULL,
                 PRIMARY KEY (track_id, position)
             );",
        )?;
        let version: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 2 {
            self.connection.execute_batch(
                "DROP TABLE IF EXISTS search_terms;
                 CREATE TABLE search_terms (
                     track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                     field TEXT NOT NULL,
                     position INTEGER NOT NULL,
                     normalized TEXT NOT NULL,
                     full_pinyin TEXT NOT NULL,
                     initials TEXT NOT NULL,
                     PRIMARY KEY (track_id, field, position)
                 );
                 PRAGMA user_version = 2;",
            )?;
            self.rebuild_search_terms()?;
        }
        if version < 3 {
            if !self.has_track_column("title_sort")? {
                self.connection.execute_batch(
                    "ALTER TABLE tracks ADD COLUMN title_sort TEXT;
                     ALTER TABLE tracks ADD COLUMN album_sort TEXT;
                     ALTER TABLE tracks ADD COLUMN artist_sort TEXT;",
                )?;
            }
            self.connection.execute("PRAGMA user_version = 3", [])?;
        }
        if version < SCHEMA_VERSION {
            self.connection.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_search_terms_field_normalized
                     ON search_terms(field, normalized);
                 CREATE INDEX IF NOT EXISTS idx_search_terms_field_full_pinyin
                     ON search_terms(field, full_pinyin);
                 CREATE INDEX IF NOT EXISTS idx_search_terms_field_initials
                     ON search_terms(field, initials);
                 PRAGMA user_version = 4;",
            )?;
        }
        Ok(())
    }

    fn has_track_column(&self, column: &str) -> Result<bool, StorageError> {
        let mut statement = self.connection.prepare("PRAGMA table_info(tracks)")?;
        let names = statement.query_map([], |row| row.get::<_, String>(1))?;
        for name in names {
            if name? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn rebuild_search_terms(&self) -> Result<(), StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, title, album FROM tracks")?;
        let tracks = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, TrackId>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        for (track_id, title, album) in tracks {
            let artists = self.artists_for_track(track_id)?;
            replace_search_terms(
                &self.connection,
                track_id,
                &title,
                &artists,
                album.as_deref(),
            )?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct TrackRow {
    id: TrackId,
    path: String,
    title: String,
    album: Option<String>,
    duration_ms: Option<i64>,
    title_sort: Option<String>,
    album_sort: Option<String>,
    artist_sort: Option<String>,
    modified_at: i64,
}

fn configure_connection(connection: &Connection) -> Result<(), StorageError> {
    connection.busy_timeout(Duration::from_millis(u64::from(BUSY_TIMEOUT_MS)))?;
    let _: String = connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn upsert_track_on(
    connection: &Connection,
    track: &TrackMetadata,
    modified_at: i64,
) -> Result<TrackId, StorageError> {
    let path = path_as_str(&track.path)?;
    let duration_ms = track
        .duration
        .map(|duration| i64::try_from(duration.as_millis()))
        .transpose()
        .map_err(|_| StorageError::DurationOverflow)?;

    connection.execute(
        "INSERT INTO tracks (
             path, title, album, duration_ms, modified_at,
             title_sort, album_sort, artist_sort
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(path) DO UPDATE SET
             title = excluded.title,
             album = excluded.album,
             duration_ms = excluded.duration_ms,
             modified_at = excluded.modified_at,
             title_sort = excluded.title_sort,
             album_sort = excluded.album_sort,
             artist_sort = excluded.artist_sort",
        params![
            path,
            track.title,
            track.album,
            duration_ms,
            modified_at,
            track.title_sort,
            track.album_sort,
            track.artist_sort
        ],
    )?;
    let track_id =
        connection.query_row("SELECT id FROM tracks WHERE path = ?1", [path], |row| {
            row.get(0)
        })?;

    connection.execute("DELETE FROM track_artists WHERE track_id = ?1", [track_id])?;
    for (position, artist) in track.artists.iter().enumerate() {
        let position = i64::try_from(position).map_err(|_| StorageError::ArtistPositionOverflow)?;
        connection.execute(
            "INSERT INTO track_artists (track_id, artist, position) VALUES (?1, ?2, ?3)",
            params![track_id, artist, position],
        )?;
    }
    replace_search_terms(
        connection,
        track_id,
        &track.title,
        &track.artists,
        track.album.as_deref(),
    )?;
    Ok(track_id)
}

fn joined_track_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(TrackRow, Option<String>)> {
    Ok((
        TrackRow {
            id: row.get(0)?,
            path: row.get(1)?,
            title: row.get(2)?,
            album: row.get(3)?,
            duration_ms: row.get(4)?,
            title_sort: row.get(5)?,
            album_sort: row.get(6)?,
            artist_sort: row.get(7)?,
            modified_at: row.get(8)?,
        },
        row.get(9)?,
    ))
}

fn collect_joined_tracks(
    rows: rusqlite::MappedRows<
        '_,
        impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<(TrackRow, Option<String>)>,
    >,
) -> Result<Vec<StoredTrack>, StorageError> {
    let mut tracks = Vec::<StoredTrack>::new();
    for row in rows {
        let (track_row, artist) = row?;
        match tracks.last_mut() {
            Some(track) if track.id == track_row.id => {
                if let Some(artist) = artist {
                    track.metadata.artists.push(artist);
                }
            }
            _ => {
                let artists = artist.into_iter().collect();
                tracks.push(stored_track(track_row, artists)?);
            }
        }
    }
    Ok(tracks)
}

fn stored_track(row: TrackRow, artists: Vec<String>) -> Result<StoredTrack, StorageError> {
    let duration = row
        .duration_ms
        .map(u64::try_from)
        .transpose()
        .map_err(|_| StorageError::DurationOverflow)?
        .map(Duration::from_millis);
    Ok(StoredTrack {
        id: row.id,
        metadata: TrackMetadata {
            path: row.path.into(),
            title: row.title,
            album: row.album,
            artists,
            duration,
            title_sort: row.title_sort,
            album_sort: row.album_sort,
            artist_sort: row.artist_sort,
            modified_at: row.modified_at,
        },
    })
}

fn replace_search_terms(
    connection: &Connection,
    track_id: TrackId,
    title: &str,
    artists: &[String],
    album: Option<&str>,
) -> Result<(), StorageError> {
    connection.execute("DELETE FROM search_terms WHERE track_id = ?1", [track_id])?;
    insert_search_term(connection, track_id, SEARCH_FIELD_TITLE, 0, title)?;
    for (position, artist) in artists.iter().enumerate() {
        let position = i64::try_from(position).map_err(|_| StorageError::ArtistPositionOverflow)?;
        insert_search_term(connection, track_id, SEARCH_FIELD_ARTIST, position, artist)?;
    }
    if let Some(album) = album {
        insert_search_term(connection, track_id, SEARCH_FIELD_ALBUM, 0, album)?;
    }
    Ok(())
}

fn insert_search_term(
    connection: &Connection,
    track_id: TrackId,
    field: &str,
    position: i64,
    value: &str,
) -> Result<(), StorageError> {
    let key = search_key(value);
    connection.execute(
        "INSERT INTO search_terms (track_id, field, position, normalized, full_pinyin, initials)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            track_id,
            field,
            position,
            key.normalized,
            key.full_pinyin,
            key.initials
        ],
    )?;
    Ok(())
}

fn path_as_str(path: &Path) -> Result<&str, StorageError> {
    path.to_str()
        .ok_or_else(|| StorageError::NonUtf8Path(path.display().to_string()))
}

fn escape_like_literal(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_the_field_search_schema() {
        let database = Database::open_in_memory().expect("in-memory database should open");
        let version = database
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .expect("schema version should be readable");

        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn upserts_lists_and_removes_a_track() {
        let mut database = Database::open_in_memory().expect("in-memory database should open");
        let mut track = TrackMetadata::from_display(
            "/music/清音.flac",
            "清音",
            Some("测试专辑".into()),
            vec!["歌手甲".into(), "歌手乙".into()],
            Some(Duration::from_secs(42)),
        );

        let first_id = database
            .upsert_track(&track, 1)
            .expect("track should be inserted");
        track.title = "清音（更新）".into();
        let second_id = database
            .upsert_track(&track, 2)
            .expect("track should be updated");

        assert_eq!(first_id, second_id);
        assert_eq!(database.track_modified_at(&track.path).unwrap(), Some(2));
        track.modified_at = 2;
        let tracks = database.list_tracks().expect("tracks should be listed");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].metadata, track);
        assert!(database.remove_track(&track.path).unwrap());
        assert!(database.list_tracks().unwrap().is_empty());
    }

    #[test]
    fn persists_optional_sort_tags() {
        let mut database = Database::open_in_memory().expect("in-memory database should open");
        let mut track = TrackMetadata::from_display(
            "/music/jay.flac",
            "晴天",
            Some("叶惠美".into()),
            vec!["周杰伦".into()],
            None,
        );
        track.title_sort = Some("Qing Tian".into());
        track.artist_sort = Some("Jay Chou".into());
        database.upsert_track(&track, 1).unwrap();

        let stored = database.list_tracks().unwrap();
        assert_eq!(stored[0].metadata.title_sort.as_deref(), Some("Qing Tian"));
        assert_eq!(stored[0].metadata.artist_sort.as_deref(), Some("Jay Chou"));
        assert!(stored[0].metadata.album_sort.is_none());
    }

    #[test]
    fn removes_tracks_under_a_directory_prefix() {
        let mut database = Database::open_in_memory().expect("in-memory database should open");
        let nested = TrackMetadata::from_display(
            "/music/album/track.flac",
            "夜色",
            Some("清音".into()),
            Vec::new(),
            None,
        );
        let sibling = TrackMetadata::from_display(
            "/music/album-live/track.flac",
            "晨光",
            None,
            Vec::new(),
            None,
        );
        let special = TrackMetadata::from_display(
            "/music/100%_live/track.flac",
            "现场",
            None,
            Vec::new(),
            None,
        );
        database.upsert_track(&nested, 1).unwrap();
        database.upsert_track(&sibling, 1).unwrap();
        database.upsert_track(&special, 1).unwrap();

        assert_eq!(database.remove_tracks_under("/music/album").unwrap(), 1);
        let remaining = database.list_tracks().unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(
            remaining
                .iter()
                .any(|track| track.metadata.path == sibling.path)
        );
        assert!(
            remaining
                .iter()
                .any(|track| track.metadata.path == special.path)
        );

        assert_eq!(database.remove_tracks_under("/music/100%_live").unwrap(), 1);
        assert_eq!(
            database.list_tracks().unwrap()[0].metadata.path,
            sibling.path
        );
        assert_eq!(database.remove_tracks_under("/").unwrap(), 0);
        assert_eq!(database.list_tracks().unwrap().len(), 1);
    }

    #[test]
    fn searches_fields_separately_and_ranks_title_first() {
        let mut database = Database::open_in_memory().expect("in-memory database should open");
        let mut title_hit = TrackMetadata::from_display(
            "/music/qingyin.flac",
            "清音",
            Some("山水之间".into()),
            vec!["测试歌手".into()],
            Some(Duration::from_secs(42)),
        );
        let mut album_hit = TrackMetadata::from_display(
            "/music/other.flac",
            "夜色",
            Some("清音".into()),
            Vec::new(),
            None,
        );
        let mut latin_track = TrackMetadata::from_display(
            "/music/rain.flac",
            "Rain",
            Some("Wait".into()),
            vec!["Love".into()],
            None,
        );
        let mut special_track =
            TrackMetadata::from_display("/music/breeze.flac", "清风 100%_", None, Vec::new(), None);
        database.upsert_track(&title_hit, 1).unwrap();
        database.upsert_track(&album_hit, 1).unwrap();
        database.upsert_track(&latin_track, 1).unwrap();
        database.upsert_track(&special_track, 1).unwrap();
        title_hit.modified_at = 1;
        album_hit.modified_at = 1;
        latin_track.modified_at = 1;
        special_track.modified_at = 1;

        for query in ["清音", "qingyin", "qy"] {
            let results = database.search_tracks(query, 10).unwrap();
            assert_eq!(
                results[0].metadata, title_hit,
                "query {query} should rank title first"
            );
            assert_eq!(
                results[1].metadata, album_hit,
                "query {query} should keep album matches"
            );
        }
        assert_eq!(
            database.search_tracks("测试歌手", 10).unwrap()[0].metadata,
            title_hit
        );
        assert_eq!(
            database.search_tracks("sszj", 10).unwrap()[0].metadata,
            title_hit
        );
        assert_eq!(
            database.search_tracks("爱", 10).unwrap().len(),
            0,
            "Han queries must not pinyin-match Latin titles"
        );
        assert_eq!(
            database.search_tracks("rain", 10).unwrap()[0].metadata,
            latin_track
        );
        assert!(database.search_tracks("不存在", 10).unwrap().is_empty());
        assert!(database.search_tracks("", 10).unwrap().is_empty());
        assert!(database.search_tracks("清音", 0).unwrap().is_empty());
        assert_eq!(database.search_tracks("清", 1).unwrap().len(), 1);
        assert_eq!(
            database.search_tracks("%_", 10).unwrap()[0].metadata,
            special_track
        );
    }

    #[test]
    fn migrates_concatenated_search_terms_to_field_documents() {
        let database = Database::open_in_memory().expect("in-memory database should open");
        database
            .connection
            .execute_batch(
                "DROP TABLE search_terms;
                 CREATE TABLE search_terms (
                     track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
                     normalized TEXT NOT NULL,
                     full_pinyin TEXT NOT NULL,
                     initials TEXT NOT NULL
                 );
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        database
            .connection
            .execute(
                "INSERT INTO tracks (path, title, album, duration_ms, modified_at)
                 VALUES ('/music/love.flac', '爱', 'Rain', NULL, 1)",
                [],
            )
            .unwrap();
        database
            .connection
            .execute(
                "INSERT INTO search_terms (track_id, normalized, full_pinyin, initials)
                 VALUES (1, '爱 rain', 'airain', 'ar')",
                [],
            )
            .unwrap();

        database.migrate().unwrap();
        let version: u32 = database
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(database.search_tracks("爱", 10).unwrap().len(), 1);
        assert_eq!(
            database.search_tracks("rain", 10).unwrap()[0]
                .metadata
                .album
                .as_deref(),
            Some("Rain")
        );
        assert_eq!(
            database.search_tracks("爱", 10).unwrap()[0].metadata.title,
            "爱"
        );
    }

    #[test]
    fn file_database_enables_wal_and_busy_timeout() {
        let directory = std::env::temp_dir().join(format!(
            "qingyin-storage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("library.sqlite3");
        let database = Database::open(&path).expect("file database should open");
        let mode: String = database
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        let timeout: i32 = database
            .connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_ascii_lowercase(), "wal");
        assert_eq!(timeout, i32::try_from(BUSY_TIMEOUT_MS).unwrap());
        drop(database);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn field_normalized_lookup_uses_search_index() {
        let database = Database::open_in_memory().unwrap();
        let mut statement = database
            .connection
            .prepare(
                "EXPLAIN QUERY PLAN
                 SELECT track_id FROM search_terms
                 WHERE field = 'title' AND normalized = 'qingyin'",
            )
            .unwrap();
        let plan = statement
            .query_map([], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join(" ");
        assert!(
            plan.to_ascii_lowercase()
                .contains("idx_search_terms_field_normalized"),
            "plan was {plan}"
        );
    }

    #[test]
    fn upserts_a_batch_of_tracks_in_one_transaction() {
        let mut database = Database::open_in_memory().unwrap();
        let first = TrackMetadata::from_display("/music/a.flac", "A", None, Vec::new(), None);
        let second = TrackMetadata::from_display("/music/b.flac", "B", None, Vec::new(), None);
        database
            .upsert_tracks(&[(&first, 1), (&second, 2)])
            .unwrap();
        let tracks = database.list_tracks().unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].metadata.title, "A");
        assert_eq!(tracks[0].metadata.modified_at, 1);
        assert_eq!(tracks[1].metadata.title, "B");
        assert_eq!(tracks[1].metadata.modified_at, 2);
    }
}
