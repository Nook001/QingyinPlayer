use std::path::{Path, PathBuf};
use std::time::Duration;

use qingyin_chinese::{contains_han, search_key};
use qingyin_metadata::{AudioProperties, FileFingerprint, TrackMetadata, unique_credited_artists};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use thiserror::Error;

pub type TrackId = i64;

const SEARCH_FIELD_TITLE: &str = "title";
const SEARCH_FIELD_ARTIST: &str = "artist";
const SEARCH_FIELD_ALBUM: &str = "album";
pub const SCHEMA_VERSION: u32 = 7;
const BUSY_TIMEOUT_MS: u32 = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTrack {
    pub id: TrackId,
    pub metadata: TrackMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRef {
    pub id: TrackId,
    pub path: PathBuf,
    pub fingerprint: FileFingerprint,
}

/// A named set of tracks. Membership has no playback order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistSummary {
    pub id: i64,
    pub name: String,
    pub track_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHits {
    pub tracks: Vec<StoredTrack>,
    pub has_more: bool,
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
    #[error("database schema version {found} is newer than supported version {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("database schema version {found} requires migration before use")]
    NeedsMigration { found: u32 },
}

impl StorageError {
    #[must_use]
    pub fn is_batch_fatal(&self) -> bool {
        match self {
            Self::Sqlite(error) => matches!(
                error.sqlite_error_code(),
                Some(
                    rusqlite::ErrorCode::DatabaseBusy
                        | rusqlite::ErrorCode::DatabaseLocked
                        | rusqlite::ErrorCode::DiskFull
                        | rusqlite::ErrorCode::SystemIoFailure
                        | rusqlite::ErrorCode::DatabaseCorrupt
                )
            ),
            Self::UnsupportedSchema { .. } | Self::NeedsMigration { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct Database {
    connection: Connection,
}

impl Database {
    /// Opens a database and applies pending schema migrations under a write lock.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot open or migrate the database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        configure_connection(&connection)?;
        let mut database = Self { connection };
        database.migrate()?;
        Ok(database)
    }

    /// Opens an existing database without migrating. Call [`Self::open`] first on startup.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the schema is missing, outdated, or too new.
    pub fn open_migrated(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        configure_connection(&connection)?;
        let database = Self { connection };
        database.assert_current_schema()?;
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
        let mut database = Self { connection };
        database.migrate()?;
        Ok(database)
    }

    /// Inserts or replaces a track and returns its stable id from the write statement.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when data conversion or an SQLite operation fails.
    pub fn upsert_track(&mut self, track: &TrackMetadata) -> Result<TrackId, StorageError> {
        let transaction = self.connection.transaction()?;
        let track_id = upsert_track_on(&transaction, track)?;
        transaction.commit()?;
        Ok(track_id)
    }

    /// Inserts or replaces many tracks in a single transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when data conversion or an SQLite operation fails.
    pub fn upsert_tracks(&mut self, tracks: &[&TrackMetadata]) -> Result<(), StorageError> {
        if tracks.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        for track in tracks {
            upsert_track_on(&transaction, track)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Returns every stored track ordered by title and path.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot execute or decode the query.
    /// Creates a playlist and returns its id. The name is stored as given.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot insert the row.
    pub fn create_playlist(&mut self, name: &str) -> Result<i64, StorageError> {
        self.connection
            .prepare_cached("INSERT INTO playlists (name) VALUES (?1)")?
            .execute(params![name])?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Renames a playlist.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot update the row.
    pub fn rename_playlist(&mut self, id: i64, name: &str) -> Result<(), StorageError> {
        self.connection
            .prepare_cached("UPDATE playlists SET name = ?1 WHERE id = ?2")?
            .execute(params![name, id])?;
        Ok(())
    }

    /// Deletes a playlist and its membership rows.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot delete the row.
    pub fn delete_playlist(&mut self, id: i64) -> Result<(), StorageError> {
        self.connection
            .prepare_cached("DELETE FROM playlists WHERE id = ?1")?
            .execute(params![id])?;
        Ok(())
    }

    /// Lists playlists by name. Track count is membership, not a play order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot read the rows.
    pub fn list_playlists(&self) -> Result<Vec<PlaylistSummary>, StorageError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT playlists.id, playlists.name, COUNT(playlist_tracks.track_id)
             FROM playlists
             LEFT JOIN playlist_tracks ON playlist_tracks.playlist_id = playlists.id
             GROUP BY playlists.id
             ORDER BY playlists.name, playlists.id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(PlaylistSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                track_count: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// Returns member track ids. The order is not a playback order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot read the rows.
    pub fn playlist_track_ids(&self, playlist_id: i64) -> Result<Vec<TrackId>, StorageError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY track_id",
        )?;
        let rows = statement.query_map(params![playlist_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// Adds a track to a playlist. A second insert of the same track is ignored.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot write the membership.
    pub fn add_playlist_track(
        &mut self,
        playlist_id: i64,
        track_id: TrackId,
    ) -> Result<(), StorageError> {
        self.connection
            .prepare_cached(
                "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id) VALUES (?1, ?2)",
            )?
            .execute(params![playlist_id, track_id])?;
        Ok(())
    }

    /// Removes one track from a playlist.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot delete the membership.
    pub fn remove_playlist_track(
        &mut self,
        playlist_id: i64,
        track_id: TrackId,
    ) -> Result<(), StorageError> {
        self.connection
            .prepare_cached("DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2")?
            .execute(params![playlist_id, track_id])?;
        Ok(())
    }

    pub fn list_tracks(&self) -> Result<Vec<StoredTrack>, StorageError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT tracks.id, tracks.path, tracks.title, tracks.album, tracks.album_artist,
                    tracks.duration_ms, tracks.disc_number, tracks.track_number,
                    tracks.title_sort, tracks.album_sort, tracks.artist_sort,
                    tracks.modified_at_ns, tracks.file_size, tracks.cover_digest,
                    tracks.audio_format, tracks.sample_rate, tracks.bit_depth, tracks.channels, tracks.bitrate,
                    track_artists.artist
             FROM tracks
             LEFT JOIN track_artists ON track_artists.track_id = tracks.id
             ORDER BY tracks.title, tracks.path, track_artists.position",
        )?;
        collect_joined_tracks(statement.query_map([], joined_track_row)?)
    }

    /// Returns path and fingerprint rows without joining artists.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot execute the query.
    pub fn list_track_refs(&self) -> Result<Vec<TrackRef>, StorageError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT id, path, modified_at_ns, file_size FROM tracks ORDER BY path",
        )?;
        statement
            .query_map([], |row| {
                Ok(TrackRef {
                    id: row.get(0)?,
                    path: PathBuf::from(row.get::<_, String>(1)?),
                    fingerprint: FileFingerprint {
                        modified_at_ns: row.get(2)?,
                        file_size: row.get::<_, i64>(3)? as u64,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// Searches stored tracks by title, artist, or album.
    ///
    /// Root filters are applied before `LIMIT`. `has_more` is true when more than `limit`
    /// matches exist.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot execute or decode the query.
    pub fn search_tracks(
        &self,
        query: &str,
        limit: usize,
        roots: &[PathBuf],
    ) -> Result<SearchHits, StorageError> {
        let query = search_key(query);
        if query.normalized.is_empty() || roots.is_empty() {
            return Ok(SearchHits {
                tracks: Vec::new(),
                has_more: false,
            });
        }
        let mut root_bounds = Vec::new();
        for root in roots {
            if let Some(bounds) = prefix_bounds(root)? {
                root_bounds.push(bounds);
            }
        }
        if root_bounds.is_empty() {
            return Ok(SearchHits {
                tracks: Vec::new(),
                has_more: false,
            });
        }
        let sql = production_search_sql(root_bounds.len());
        let mut statement = self.connection.prepare_cached(&sql)?;
        let params = bind_search_params(&query, limit, &root_bounds);
        let tracks = collect_joined_tracks(
            statement.query_map(rusqlite::params_from_iter(params), joined_track_row)?,
        )?;
        let has_more = tracks.len() > limit;
        Ok(SearchHits {
            tracks: tracks.into_iter().take(limit).collect(),
            has_more,
        })
    }

    /// Explains the production substring search plan for tests and measurement.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot explain the query.
    pub fn explain_search_plan(
        &self,
        query: &str,
        roots: &[PathBuf],
    ) -> Result<String, StorageError> {
        let query = search_key(query);
        let mut root_bounds = Vec::new();
        for root in roots {
            if let Some(bounds) = prefix_bounds(root)? {
                root_bounds.push(bounds);
            }
        }
        if query.normalized.is_empty() || root_bounds.is_empty() {
            return Ok(String::new());
        }
        let sql = format!(
            "EXPLAIN QUERY PLAN {}",
            production_search_sql(root_bounds.len())
        );
        let mut statement = self.connection.prepare(&sql)?;
        let params = bind_search_params(&query, 10, &root_bounds);
        let plan = statement
            .query_map(rusqlite::params_from_iter(params), |row| {
                row.get::<_, String>(3)
            })?
            .collect::<Result<Vec<_>, _>>()?
            .join(" ");
        Ok(plan)
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

    /// Removes the given ids in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot delete the rows.
    pub fn remove_track_ids(&mut self, ids: &[TrackId]) -> Result<usize, StorageError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let transaction = self.connection.transaction()?;
        let mut deleted = 0;
        {
            let mut statement = transaction.prepare("DELETE FROM tracks WHERE id = ?1")?;
            for id in ids {
                deleted += statement.execute(params![id])?;
            }
        }
        transaction.commit()?;
        Ok(deleted)
    }

    /// Removes every track stored at `directory` or nested under it.
    ///
    /// Matching is case-sensitive and requires a path-separator boundary. The root
    /// filesystem path is ignored so a bad event cannot wipe the library.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the path is invalid or SQLite cannot delete the rows.
    pub fn remove_tracks_under(&self, directory: impl AsRef<Path>) -> Result<usize, StorageError> {
        let Some((directory, prefix)) = prefix_bounds(directory.as_ref())? else {
            return Ok(0);
        };
        let deleted = self.connection.execute(
            "DELETE FROM tracks WHERE path = ?1 OR path GLOB ?2",
            params![directory, glob_prefix(&prefix)],
        )?;
        Ok(deleted)
    }

    /// Lists ids and paths stored at or under `directory`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the path is invalid or SQLite cannot query.
    pub fn list_refs_under(
        &self,
        directory: impl AsRef<Path>,
    ) -> Result<Vec<TrackRef>, StorageError> {
        let Some((directory, prefix)) = prefix_bounds(directory.as_ref())? else {
            return Ok(Vec::new());
        };
        let mut statement = self.connection.prepare_cached(
            "SELECT id, path, modified_at_ns, file_size FROM tracks
             WHERE path = ?1 OR path GLOB ?2
             ORDER BY path",
        )?;
        statement
            .query_map(params![directory, glob_prefix(&prefix)], |row| {
                Ok(TrackRef {
                    id: row.get(0)?,
                    path: PathBuf::from(row.get::<_, String>(1)?),
                    fingerprint: FileFingerprint {
                        modified_at_ns: row.get(2)?,
                        file_size: row.get::<_, i64>(3)? as u64,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// Returns the fingerprint recorded for a path, if present.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the path is invalid or SQLite cannot execute the query.
    pub fn track_fingerprint(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<Option<FileFingerprint>, StorageError> {
        let path = path_as_str(path.as_ref())?;
        self.connection
            .query_row(
                "SELECT modified_at_ns, file_size FROM tracks WHERE path = ?1",
                [path],
                |row| {
                    Ok(FileFingerprint {
                        modified_at_ns: row.get(0)?,
                        file_size: row.get::<_, i64>(1)? as u64,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn assert_current_schema(&self) -> Result<(), StorageError> {
        let version = user_version(&self.connection)?;
        if version > SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema {
                found: version,
                supported: SCHEMA_VERSION,
            });
        }
        if version != SCHEMA_VERSION {
            return Err(StorageError::NeedsMigration { found: version });
        }
        Ok(())
    }

    fn migrate(&mut self) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version = user_version(&transaction)?;
        if version > SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema {
                found: version,
                supported: SCHEMA_VERSION,
            });
        }
        if version == 0 {
            transaction.execute_batch(LATEST_SCHEMA)?;
            set_user_version(&transaction, SCHEMA_VERSION)?;
            transaction.commit()?;
            return Ok(());
        }
        if version < 2 {
            transaction.execute_batch(
                "DROP TABLE IF EXISTS search_terms;
                 CREATE TABLE search_terms (
                     track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                     field TEXT NOT NULL,
                     position INTEGER NOT NULL,
                     normalized TEXT NOT NULL,
                     full_pinyin TEXT NOT NULL,
                     initials TEXT NOT NULL,
                     PRIMARY KEY (track_id, field, position)
                 );",
            )?;
            rebuild_search_terms(&transaction)?;
            set_user_version(&transaction, 2)?;
        }
        if version < 3 {
            if !has_track_column(&transaction, "title_sort")? {
                transaction.execute_batch(
                    "ALTER TABLE tracks ADD COLUMN title_sort TEXT;
                     ALTER TABLE tracks ADD COLUMN album_sort TEXT;
                     ALTER TABLE tracks ADD COLUMN artist_sort TEXT;",
                )?;
            }
            set_user_version(&transaction, 3)?;
        }
        if version < 4 {
            transaction.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_search_terms_field_normalized
                     ON search_terms(field, normalized);
                 CREATE INDEX IF NOT EXISTS idx_search_terms_field_full_pinyin
                     ON search_terms(field, full_pinyin);
                 CREATE INDEX IF NOT EXISTS idx_search_terms_field_initials
                     ON search_terms(field, initials);",
            )?;
            set_user_version(&transaction, 4)?;
        }
        if version < 5 {
            if !has_track_column(&transaction, "modified_at_ns")? {
                transaction.execute_batch(
                    "ALTER TABLE tracks ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0;
                     ALTER TABLE tracks ADD COLUMN modified_at_ns INTEGER NOT NULL DEFAULT 0;
                     ALTER TABLE tracks ADD COLUMN album_artist TEXT;
                     ALTER TABLE tracks ADD COLUMN disc_number INTEGER;
                     ALTER TABLE tracks ADD COLUMN track_number INTEGER;
                     ALTER TABLE tracks ADD COLUMN cover_digest TEXT;",
                )?;
            }
            transaction.execute(
                "UPDATE tracks SET modified_at_ns = modified_at * 1000000000
                 WHERE modified_at_ns = 0 AND modified_at > 0",
                [],
            )?;
            set_user_version(&transaction, 5)?;
        }
        if version < 6 {
            if !has_track_column(&transaction, "audio_format")? {
                transaction.execute_batch(
                    "ALTER TABLE tracks ADD COLUMN audio_format TEXT;
                     ALTER TABLE tracks ADD COLUMN sample_rate INTEGER;
                     ALTER TABLE tracks ADD COLUMN bit_depth INTEGER;
                     ALTER TABLE tracks ADD COLUMN channels INTEGER;
                     ALTER TABLE tracks ADD COLUMN bitrate INTEGER;
                     UPDATE tracks SET modified_at_ns = 0;",
                )?;
            }
            set_user_version(&transaction, 6)?;
        }
        if version < 7 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS playlists (
                     id INTEGER PRIMARY KEY,
                     name TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS playlist_tracks (
                     playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                     track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                     PRIMARY KEY (playlist_id, track_id)
                 );",
            )?;
            set_user_version(&transaction, 7)?;
        }
        transaction.commit()?;
        Ok(())
    }
}

const LATEST_SCHEMA: &str = "
    PRAGMA foreign_keys = ON;
    CREATE TABLE IF NOT EXISTS tracks (
        id INTEGER PRIMARY KEY,
        path TEXT NOT NULL UNIQUE,
        title TEXT NOT NULL,
        album TEXT,
        album_artist TEXT,
        duration_ms INTEGER,
        disc_number INTEGER,
        track_number INTEGER,
        modified_at INTEGER NOT NULL DEFAULT 0,
        modified_at_ns INTEGER NOT NULL DEFAULT 0,
        file_size INTEGER NOT NULL DEFAULT 0,
        title_sort TEXT,
        album_sort TEXT,
        artist_sort TEXT,
        cover_digest TEXT,
        audio_format TEXT,
        sample_rate INTEGER,
        bit_depth INTEGER,
        channels INTEGER,
        bitrate INTEGER
    );
    CREATE TABLE IF NOT EXISTS track_artists (
        track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        artist TEXT NOT NULL,
        position INTEGER NOT NULL,
        PRIMARY KEY (track_id, position)
    );
    CREATE TABLE IF NOT EXISTS search_terms (
        track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        field TEXT NOT NULL,
        position INTEGER NOT NULL,
        normalized TEXT NOT NULL,
        full_pinyin TEXT NOT NULL,
        initials TEXT NOT NULL,
        PRIMARY KEY (track_id, field, position)
    );
    CREATE TABLE IF NOT EXISTS playlists (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS playlist_tracks (
        playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
        track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        PRIMARY KEY (playlist_id, track_id)
    );
    CREATE INDEX IF NOT EXISTS idx_search_terms_field_normalized
        ON search_terms(field, normalized);
    CREATE INDEX IF NOT EXISTS idx_search_terms_field_full_pinyin
        ON search_terms(field, full_pinyin);
    CREATE INDEX IF NOT EXISTS idx_search_terms_field_initials
        ON search_terms(field, initials);
";

#[derive(Debug)]
struct TrackRow {
    id: TrackId,
    path: String,
    title: String,
    album: Option<String>,
    album_artist: Option<String>,
    duration_ms: Option<i64>,
    disc_number: Option<i64>,
    track_number: Option<i64>,
    title_sort: Option<String>,
    album_sort: Option<String>,
    artist_sort: Option<String>,
    modified_at_ns: i64,
    file_size: i64,
    cover_digest: Option<String>,
    audio: AudioProperties,
}

fn configure_connection(connection: &Connection) -> Result<(), StorageError> {
    connection.busy_timeout(Duration::from_millis(u64::from(BUSY_TIMEOUT_MS)))?;
    let _: String = connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn user_version(connection: &Connection) -> Result<u32, StorageError> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(StorageError::from)
}

fn set_user_version(connection: &Connection, version: u32) -> Result<(), StorageError> {
    connection
        .pragma_update(None, "user_version", version)
        .map_err(StorageError::from)
}

fn has_track_column(connection: &Connection, column: &str) -> Result<bool, StorageError> {
    let mut statement = connection.prepare("PRAGMA table_info(tracks)")?;
    let names = statement.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn rebuild_search_terms(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare("SELECT id, title, album FROM tracks")?;
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
        let mut artist_statement = connection
            .prepare("SELECT artist FROM track_artists WHERE track_id = ?1 ORDER BY position")?;
        let artists = unique_credited_artists(
            artist_statement
                .query_map([track_id], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?,
        );
        replace_search_terms(connection, track_id, &title, &artists, album.as_deref())?;
    }
    Ok(())
}

fn upsert_track_on(
    connection: &Connection,
    track: &TrackMetadata,
) -> Result<TrackId, StorageError> {
    let path = path_as_str(&track.path)?;
    let duration_ms = track
        .duration
        .map(|duration| i64::try_from(duration.as_millis()))
        .transpose()
        .map_err(|_| StorageError::DurationOverflow)?;
    let file_size = i64::try_from(track.file_size).unwrap_or(i64::MAX);
    let seconds = track.modified_at_ns / 1_000_000_000;

    let mut statement = connection.prepare_cached(
        "INSERT INTO tracks (
             path, title, album, album_artist, duration_ms, disc_number, track_number,
             modified_at, modified_at_ns, file_size,
             title_sort, album_sort, artist_sort, cover_digest,
             audio_format, sample_rate, bit_depth, channels, bitrate
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
         ON CONFLICT(path) DO UPDATE SET
             title = excluded.title,
             album = excluded.album,
             album_artist = excluded.album_artist,
             duration_ms = excluded.duration_ms,
             disc_number = excluded.disc_number,
             track_number = excluded.track_number,
             modified_at = excluded.modified_at,
             modified_at_ns = excluded.modified_at_ns,
             file_size = excluded.file_size,
             title_sort = excluded.title_sort,
             album_sort = excluded.album_sort,
             artist_sort = excluded.artist_sort,
             cover_digest = excluded.cover_digest,
             audio_format = excluded.audio_format,
             sample_rate = excluded.sample_rate,
             bit_depth = excluded.bit_depth,
             channels = excluded.channels,
             bitrate = excluded.bitrate
         RETURNING id",
    )?;
    let track_id = statement.query_row(
        params![
            path,
            track.title,
            track.album,
            track.album_artist,
            duration_ms,
            track.disc_number.map(i64::from),
            track.track_number.map(i64::from),
            seconds,
            track.modified_at_ns,
            file_size,
            track.title_sort,
            track.album_sort,
            track.artist_sort,
            track.cover_digest,
            track.audio.format,
            track.audio.sample_rate,
            track.audio.bit_depth,
            track.audio.channels,
            track.audio.bitrate,
        ],
        |row| row.get(0),
    )?;
    drop(statement);

    connection.execute("DELETE FROM track_artists WHERE track_id = ?1", [track_id])?;
    let artists = unique_credited_artists(track.artists.iter());
    let mut artist_statement = connection.prepare_cached(
        "INSERT INTO track_artists (track_id, artist, position) VALUES (?1, ?2, ?3)",
    )?;
    for (position, artist) in artists.iter().enumerate() {
        let position = i64::try_from(position).map_err(|_| StorageError::ArtistPositionOverflow)?;
        artist_statement.execute(params![track_id, artist, position])?;
    }
    drop(artist_statement);
    replace_search_terms(
        connection,
        track_id,
        &track.title,
        &artists,
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
            album_artist: row.get(4)?,
            duration_ms: row.get(5)?,
            disc_number: row.get(6)?,
            track_number: row.get(7)?,
            title_sort: row.get(8)?,
            album_sort: row.get(9)?,
            artist_sort: row.get(10)?,
            modified_at_ns: row.get(11)?,
            file_size: row.get(12)?,
            cover_digest: row.get(13)?,
            audio: AudioProperties {
                format: row.get(14)?,
                sample_rate: row.get(15)?,
                bit_depth: row.get(16)?,
                channels: row.get(17)?,
                bitrate: row.get(18)?,
            },
        },
        row.get(19)?,
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
    for track in &mut tracks {
        track.metadata.artists = unique_credited_artists(track.metadata.artists.iter());
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
            id: row.id,
            title: row.title,
            album: row.album,
            album_artist: row.album_artist,
            artists,
            duration,
            disc_number: row.disc_number.and_then(|value| u32::try_from(value).ok()),
            track_number: row.track_number.and_then(|value| u32::try_from(value).ok()),
            title_sort: row.title_sort,
            album_sort: row.album_sort,
            artist_sort: row.artist_sort,
            file_size: u64::try_from(row.file_size).unwrap_or(0),
            modified_at_ns: row.modified_at_ns,
            cover_digest: row.cover_digest,
            audio: row.audio,
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
    let mut statement = connection.prepare_cached(
        "INSERT INTO search_terms (track_id, field, position, normalized, full_pinyin, initials)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    statement.execute(params![
        track_id,
        field,
        position,
        key.normalized,
        key.full_pinyin,
        key.initials
    ])?;
    Ok(())
}

fn path_as_str(path: &Path) -> Result<&str, StorageError> {
    path.to_str()
        .ok_or_else(|| StorageError::NonUtf8Path(path.display().to_string()))
}

fn prefix_bounds(directory: &Path) -> Result<Option<(String, String)>, StorageError> {
    let directory = path_as_str(directory)?.trim_end_matches('/');
    if directory.is_empty() || directory == "/" {
        return Ok(None);
    }
    Ok(Some((directory.to_owned(), format!("{directory}/"))))
}

fn production_search_sql(root_count: usize) -> String {
    format!(
        "SELECT ranked.id, ranked.path, ranked.title, ranked.album, ranked.album_artist,
                ranked.duration_ms, ranked.disc_number, ranked.track_number,
                ranked.title_sort, ranked.album_sort, ranked.artist_sort,
                ranked.modified_at_ns, ranked.file_size, ranked.cover_digest,
                    ranked.audio_format, ranked.sample_rate, ranked.bit_depth, ranked.channels, ranked.bitrate,
                track_artists.artist
         FROM (
             SELECT tracks.id, tracks.path, tracks.title, tracks.album, tracks.album_artist,
                    tracks.duration_ms, tracks.disc_number, tracks.track_number,
                    tracks.title_sort, tracks.album_sort, tracks.artist_sort,
                    tracks.modified_at_ns, tracks.file_size, tracks.cover_digest,
                    tracks.audio_format, tracks.sample_rate, tracks.bit_depth, tracks.channels, tracks.bitrate,
                    MIN(CASE search_terms.field
                            WHEN 'title' THEN 0
                            WHEN 'artist' THEN 1
                            WHEN 'album' THEN 2
                            ELSE 3
                        END) AS match_rank
             FROM tracks
             JOIN search_terms ON search_terms.track_id = tracks.id
             WHERE (instr(search_terms.normalized, ?1) > 0
                OR (?5 = 0 AND ?2 <> '' AND instr(search_terms.full_pinyin, ?2) > 0)
                OR (?5 = 0 AND ?3 <> '' AND instr(search_terms.initials, ?3) > 0))
               AND ({})
             GROUP BY tracks.id
             ORDER BY match_rank, tracks.title, tracks.path
             LIMIT ?4
         ) AS ranked
         LEFT JOIN track_artists ON track_artists.track_id = ranked.id
         ORDER BY ranked.match_rank, ranked.title, ranked.path, track_artists.position",
        root_predicate(root_count)
    )
}

fn bind_search_params(
    query: &qingyin_chinese::SearchKey,
    limit: usize,
    root_bounds: &[(String, String)],
) -> Vec<Value> {
    let han_query = i64::from(contains_han(&query.normalized));
    let fetch = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    let mut params: Vec<Value> = vec![
        query.normalized.clone().into(),
        query.full_pinyin.clone().into(),
        query.initials.clone().into(),
        fetch.into(),
        han_query.into(),
    ];
    for (directory, prefix) in root_bounds {
        params.push(directory.clone().into());
        params.push(glob_prefix(prefix).into());
    }
    params
}

fn glob_prefix(prefix: &str) -> String {
    let mut pattern = String::new();
    for character in prefix.chars() {
        if matches!(character, '*' | '?' | '[' | ']') {
            pattern.push('[');
            pattern.push(character);
            pattern.push(']');
        } else {
            pattern.push(character);
        }
    }
    pattern.push('*');
    pattern
}

fn root_predicate(count: usize) -> String {
    root_predicate_at(6, count)
}

fn root_predicate_at(start: usize, count: usize) -> String {
    (0..count)
        .map(|index| {
            let path_idx = start + index * 2;
            let prefix_idx = path_idx + 1;
            format!("(tracks.path = ?{path_idx} OR tracks.path GLOB ?{prefix_idx})")
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_properties_survive_upsert_list_and_search() {
        let mut db = Database::open_in_memory().unwrap();
        let mut track = TrackMetadata::from_display(
            "/music/song.flac",
            "song",
            None,
            vec!["artist".into()],
            None,
        );
        track.audio = AudioProperties {
            format: Some("FLAC".into()),
            sample_rate: Some(96000),
            bit_depth: Some(24),
            channels: Some(2),
            bitrate: Some(1400),
        };
        let id = db.upsert_track(&track).unwrap();
        assert_eq!(db.list_tracks().unwrap()[0].metadata.audio, track.audio);
        assert_eq!(
            db.search_tracks("song", 10, &["/music".into()])
                .unwrap()
                .tracks[0]
                .metadata
                .audio,
            track.audio
        );
        track.audio = AudioProperties::default();
        assert_eq!(db.upsert_track(&track).unwrap(), id);
        assert_eq!(db.list_tracks().unwrap()[0].metadata.audio, track.audio);
    }

    #[test]
    fn migrates_v5_and_marks_existing_files_for_property_backfill() {
        let mut db = Database::open_in_memory().unwrap();
        let mut track = TrackMetadata::from_display("/music/song.flac", "song", None, vec![], None);
        track.modified_at_ns = 123456789;
        let id = db.upsert_track(&track).unwrap();
        db.connection
            .execute_batch(
                "ALTER TABLE tracks DROP COLUMN audio_format;
            ALTER TABLE tracks DROP COLUMN sample_rate; ALTER TABLE tracks DROP COLUMN bit_depth;
            ALTER TABLE tracks DROP COLUMN channels; ALTER TABLE tracks DROP COLUMN bitrate;
            PRAGMA user_version = 5;",
            )
            .unwrap();
        db.migrate().unwrap();
        let stored = db.list_tracks().unwrap();
        assert_eq!(stored[0].id, id);
        assert_eq!(stored[0].metadata.modified_at_ns, 0);
        assert_eq!(stored[0].metadata.audio, AudioProperties::default());
        assert_eq!(user_version(&db.connection).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn playlist_membership_is_a_set_without_play_order() {
        let mut database = Database::open_in_memory().unwrap();
        let first = TrackMetadata::from_display("/music/b.flac", "B", None, vec![], None);
        let second = TrackMetadata::from_display("/music/a.flac", "A", None, vec![], None);
        let first_id = database.upsert_track(&first).unwrap();
        let second_id = database.upsert_track(&second).unwrap();
        let playlist_id = database.create_playlist("夜").unwrap();
        database.add_playlist_track(playlist_id, first_id).unwrap();
        database.add_playlist_track(playlist_id, first_id).unwrap();
        database.add_playlist_track(playlist_id, second_id).unwrap();
        let mut members = database.playlist_track_ids(playlist_id).unwrap();
        members.sort_unstable();
        let mut expected = vec![first_id, second_id];
        expected.sort_unstable();
        assert_eq!(members, expected);
        let listed = database.list_playlists().unwrap();
        assert_eq!(listed[0].track_count, 2);
        database.remove_track(Path::new("/music/b.flac")).unwrap();
        assert_eq!(
            database.playlist_track_ids(playlist_id).unwrap(),
            vec![second_id]
        );
    }

    #[test]
    fn creates_the_field_search_schema() {
        let database = Database::open_in_memory().expect("in-memory database should open");
        let version =
            user_version(&database.connection).expect("schema version should be readable");
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
        track.modified_at_ns = 1_000_000_000;
        track.file_size = 12;

        let first_id = database
            .upsert_track(&track)
            .expect("track should be inserted");
        track.title = "清音（更新）".into();
        track.modified_at_ns = 2_000_000_000;
        let second_id = database
            .upsert_track(&track)
            .expect("track should be updated");

        assert_eq!(first_id, second_id);
        assert_eq!(
            database.track_fingerprint(&track.path).unwrap(),
            Some(FileFingerprint {
                modified_at_ns: 2_000_000_000,
                file_size: 12
            })
        );
        track.id = first_id;
        let tracks = database.list_tracks().expect("tracks should be listed");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].metadata, track);
        assert!(database.remove_track(&track.path).unwrap());
        assert!(database.list_tracks().unwrap().is_empty());
    }

    #[test]
    fn splits_joined_artist_credits_when_storing_and_listing() {
        let mut database = Database::open_in_memory().expect("in-memory database should open");
        let track = TrackMetadata::from_display(
            "/music/duet.flac",
            "合唱",
            Some("专辑".into()),
            vec!["Singer A、Singer B".into(), "or3o".into(), "OR3O".into()],
            Some(Duration::from_secs(12)),
        );
        database.upsert_track(&track).unwrap();
        let listed = database.list_tracks().unwrap();
        assert_eq!(
            listed[0].metadata.artists,
            vec![
                "Singer A".to_owned(),
                "Singer B".to_owned(),
                "OR3O".to_owned()
            ]
        );
    }

    #[test]
    fn persists_optional_sort_tags_and_album_fields() {
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
        track.album_artist = Some("周杰伦".into());
        track.disc_number = Some(1);
        track.track_number = Some(3);
        database.upsert_track(&track).unwrap();

        let stored = database.list_tracks().unwrap();
        assert_eq!(stored[0].metadata.title_sort.as_deref(), Some("Qing Tian"));
        assert_eq!(stored[0].metadata.artist_sort.as_deref(), Some("Jay Chou"));
        assert_eq!(stored[0].metadata.album_artist.as_deref(), Some("周杰伦"));
        assert_eq!(stored[0].metadata.disc_number, Some(1));
        assert_eq!(stored[0].metadata.track_number, Some(3));
        assert!(stored[0].metadata.album_sort.is_none());
    }

    #[test]
    fn removes_tracks_under_a_directory_prefix() {
        let mut database = Database::open_in_memory().expect("in-memory database should open");
        for (path, title) in [
            ("/music/album/track.flac", "夜色"),
            ("/music/album-live/track.flac", "晨光"),
            ("/music/100%_live/track.flac", "现场"),
            ("/music/Album/case.flac", "大小写"),
            ("/music/a/x.flac", "短"),
            ("/music/ab/x.flac", "长"),
            ("/music/中文/[song].flac", "中文"),
        ] {
            database
                .upsert_track(&TrackMetadata::from_display(
                    path,
                    title,
                    None,
                    Vec::new(),
                    None,
                ))
                .unwrap();
        }

        assert_eq!(database.remove_tracks_under("/music/album").unwrap(), 1);
        assert_eq!(database.remove_tracks_under("/music/Album").unwrap(), 1);
        assert_eq!(database.remove_tracks_under("/music/a").unwrap(), 1);
        assert_eq!(database.list_refs_under("/music/ab").unwrap().len(), 1);
        assert_eq!(database.remove_tracks_under("/music/100%_live").unwrap(), 1);
        assert_eq!(database.remove_tracks_under("/music/中文").unwrap(), 1);
        let remaining = database.list_tracks().unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(
            remaining
                .iter()
                .any(|track| track.metadata.path == Path::new("/music/album-live/track.flac"))
        );
        assert_eq!(database.remove_tracks_under("/").unwrap(), 0);
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
        let outside =
            TrackMetadata::from_display("/other/skip.flac", "清音", None, Vec::new(), None);
        database.upsert_track(&title_hit).unwrap();
        database.upsert_track(&album_hit).unwrap();
        database.upsert_track(&latin_track).unwrap();
        database.upsert_track(&special_track).unwrap();
        database.upsert_track(&outside).unwrap();
        title_hit.id = 1;
        album_hit.id = 2;
        latin_track.id = 3;
        special_track.id = 4;
        let roots = [PathBuf::from("/music")];

        for query in ["清音", "qingyin", "qy"] {
            let results = database.search_tracks(query, 10, &roots).unwrap();
            assert!(!results.has_more);
            assert_eq!(
                results.tracks[0].metadata.title, title_hit.title,
                "query {query} should rank title first"
            );
            assert_eq!(results.tracks[1].metadata.title, album_hit.title);
        }
        let truncated = database.search_tracks("清", 1, &roots).unwrap();
        assert!(truncated.has_more);
        assert_eq!(truncated.tracks.len(), 1);
        assert!(
            database
                .search_tracks("清音", 10, &[])
                .unwrap()
                .tracks
                .is_empty()
        );
        assert_eq!(
            database.search_tracks("%_", 10, &roots).unwrap().tracks[0]
                .metadata
                .title,
            special_track.title
        );
    }

    #[test]
    fn migrates_concatenated_search_terms_to_field_documents() {
        let mut database = Database::open_in_memory().expect("in-memory database should open");
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
        let version = user_version(&database.connection).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let roots = [PathBuf::from("/music")];
        assert_eq!(
            database
                .search_tracks("爱", 10, &roots)
                .unwrap()
                .tracks
                .len(),
            1
        );
        assert_eq!(
            database.search_tracks("rain", 10, &roots).unwrap().tracks[0]
                .metadata
                .album
                .as_deref(),
            Some("Rain")
        );
        let stored = database.list_tracks().unwrap();
        assert_eq!(stored[0].metadata.modified_at_ns, 1_000_000_000);
    }

    #[test]
    fn rejects_future_schema_versions() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .connection
            .execute("PRAGMA user_version = 99", [])
            .unwrap();
        match database.migrate() {
            Err(StorageError::UnsupportedSchema {
                found: 99,
                supported: SCHEMA_VERSION,
            }) => {}
            other => panic!("unexpected {other:?}"),
        }
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
        let reopened = Database::open_migrated(&path).unwrap();
        drop(reopened);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn production_search_sql_is_explained() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .upsert_track(&TrackMetadata::from_display(
                "/music/qingyin.flac",
                "清音",
                None,
                Vec::new(),
                None,
            ))
            .unwrap();
        let plan = database
            .explain_search_plan("qingyin", &[PathBuf::from("/music")])
            .unwrap();
        assert!(
            plan.to_ascii_lowercase().contains("search_terms")
                || plan.to_ascii_lowercase().contains("tracks"),
            "plan was {plan}"
        );
    }

    #[test]
    fn upserts_a_batch_of_tracks_in_one_transaction() {
        let mut database = Database::open_in_memory().unwrap();
        let mut first = TrackMetadata::from_display("/music/a.flac", "A", None, Vec::new(), None);
        first.modified_at_ns = 1;
        let mut second = TrackMetadata::from_display("/music/b.flac", "B", None, Vec::new(), None);
        second.modified_at_ns = 2;
        database.upsert_tracks(&[&first, &second]).unwrap();
        let tracks = database.list_tracks().unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].metadata.title, "A");
        assert_eq!(tracks[0].metadata.modified_at_ns, 1);
        assert_eq!(tracks[1].metadata.title, "B");
        assert_eq!(tracks[1].metadata.modified_at_ns, 2);
    }

    #[test]
    fn batch_deletes_ids_in_one_transaction() {
        let mut database = Database::open_in_memory().unwrap();
        let first = database
            .upsert_track(&TrackMetadata::from_display(
                "/music/a.flac",
                "A",
                None,
                Vec::new(),
                None,
            ))
            .unwrap();
        let second = database
            .upsert_track(&TrackMetadata::from_display(
                "/music/b.flac",
                "B",
                None,
                Vec::new(),
                None,
            ))
            .unwrap();
        assert_eq!(database.remove_track_ids(&[first, second]).unwrap(), 2);
        assert!(database.list_tracks().unwrap().is_empty());
    }
}
