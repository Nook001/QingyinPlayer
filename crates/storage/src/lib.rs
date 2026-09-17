use std::path::Path;
use std::time::Duration;

use qingyin_chinese::SearchKey;
use qingyin_metadata::TrackMetadata;
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

pub type TrackId = i64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTrack {
    pub id: TrackId,
    pub metadata: TrackMetadata,
    pub search_key: SearchKey,
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
        search_key: &SearchKey,
        modified_at: i64,
    ) -> Result<TrackId, StorageError> {
        let path = path_as_str(&track.path)?;
        let duration_ms = track
            .duration
            .map(|duration| i64::try_from(duration.as_millis()))
            .transpose()
            .map_err(|_| StorageError::DurationOverflow)?;
        let transaction = self.connection.transaction()?;

        transaction.execute(
            "INSERT INTO tracks (path, title, album, duration_ms, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                 title = excluded.title,
                 album = excluded.album,
                 duration_ms = excluded.duration_ms,
                 modified_at = excluded.modified_at",
            params![path, track.title, track.album, duration_ms, modified_at],
        )?;
        let track_id =
            transaction.query_row("SELECT id FROM tracks WHERE path = ?1", [path], |row| {
                row.get(0)
            })?;

        transaction.execute("DELETE FROM track_artists WHERE track_id = ?1", [track_id])?;
        for (position, artist) in track.artists.iter().enumerate() {
            let position =
                i64::try_from(position).map_err(|_| StorageError::ArtistPositionOverflow)?;
            transaction.execute(
                "INSERT INTO track_artists (track_id, artist, position) VALUES (?1, ?2, ?3)",
                params![track_id, artist, position],
            )?;
        }
        transaction.execute(
            "INSERT INTO search_terms (track_id, normalized, full_pinyin, initials)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(track_id) DO UPDATE SET
                 normalized = excluded.normalized,
                 full_pinyin = excluded.full_pinyin,
                 initials = excluded.initials",
            params![
                track_id,
                search_key.normalized,
                search_key.full_pinyin,
                search_key.initials
            ],
        )?;
        transaction.commit()?;

        Ok(track_id)
    }

    /// Returns every stored track ordered by title and path.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when SQLite cannot execute or decode the query.
    pub fn list_tracks(&self) -> Result<Vec<StoredTrack>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT tracks.id, tracks.path, tracks.title, tracks.album, tracks.duration_ms,
                    search_terms.normalized, search_terms.full_pinyin, search_terms.initials
             FROM tracks
             JOIN search_terms ON search_terms.track_id = tracks.id
             ORDER BY tracks.title, tracks.path",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, TrackId>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;
        let mut tracks = Vec::new();

        for row in rows {
            let (id, path, title, album, duration_ms, normalized, full_pinyin, initials) = row?;
            let mut artist_statement = self.connection.prepare(
                "SELECT artist FROM track_artists WHERE track_id = ?1 ORDER BY position",
            )?;
            let artists = artist_statement
                .query_map([id], |artist_row| artist_row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;

            let duration = duration_ms
                .map(u64::try_from)
                .transpose()
                .map_err(|_| StorageError::DurationOverflow)?
                .map(Duration::from_millis);
            tracks.push(StoredTrack {
                id,
                metadata: TrackMetadata {
                    path: path.into(),
                    title,
                    album,
                    artists,
                    duration,
                },
                search_key: SearchKey {
                    normalized,
                    full_pinyin,
                    initials,
                },
            });
        }

        Ok(tracks)
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

    fn migrate(&self) -> Result<(), StorageError> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS tracks (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL UNIQUE,
                 title TEXT NOT NULL,
                 album TEXT,
                 duration_ms INTEGER,
                 modified_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS track_artists (
                 track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                 artist TEXT NOT NULL,
                 position INTEGER NOT NULL,
                 PRIMARY KEY (track_id, position)
             );
             CREATE TABLE IF NOT EXISTS search_terms (
                 track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
                 normalized TEXT NOT NULL,
                 full_pinyin TEXT NOT NULL,
                 initials TEXT NOT NULL
             );
             PRAGMA user_version = 1;",
        )?;
        Ok(())
    }
}

fn path_as_str(path: &Path) -> Result<&str, StorageError> {
    path.to_str()
        .ok_or_else(|| StorageError::NonUtf8Path(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingyin_chinese::search_key;

    #[test]
    fn creates_the_initial_schema() {
        let database = Database::open_in_memory().expect("in-memory database should open");
        let version = database
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .expect("schema version should be readable");

        assert_eq!(version, 1);
    }

    #[test]
    fn upserts_lists_and_removes_a_track() {
        let mut database = Database::open_in_memory().expect("in-memory database should open");
        let mut track = TrackMetadata {
            path: "/music/清音.flac".into(),
            title: "清音".into(),
            album: Some("测试专辑".into()),
            artists: vec!["歌手甲".into(), "歌手乙".into()],
            duration: Some(Duration::from_secs(42)),
        };

        let first_id = database
            .upsert_track(&track, &search_key(&track.title), 1)
            .expect("track should be inserted");
        track.title = "清音（更新）".into();
        let second_id = database
            .upsert_track(&track, &search_key(&track.title), 2)
            .expect("track should be updated");

        assert_eq!(first_id, second_id);
        assert_eq!(database.track_modified_at(&track.path).unwrap(), Some(2));
        let tracks = database.list_tracks().expect("tracks should be listed");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].metadata, track);
        assert!(database.remove_track(&track.path).unwrap());
        assert!(database.list_tracks().unwrap().is_empty());
    }
}
