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
        replace_search_terms(
            &transaction,
            track_id,
            &track.title,
            &track.artists,
            track.album.as_deref(),
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
            "SELECT id, path, title, album, duration_ms
             FROM tracks
             ORDER BY title, path",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, TrackId>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })?;
        let mut tracks = Vec::new();

        for row in rows {
            let (id, path, title, album, duration_ms) = row?;
            let artists = self.artists_for_track(id)?;
            tracks.push(stored_track(id, path, title, album, duration_ms, artists)?);
        }

        Ok(tracks)
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
            "SELECT tracks.id, tracks.path, tracks.title, tracks.album, tracks.duration_ms,
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
             LIMIT ?4",
        )?;
        let rows = statement.query_map(
            params![
                query.normalized,
                query.full_pinyin,
                query.initials,
                i64::try_from(limit).unwrap_or(i64::MAX),
                han_query
            ],
            |row| {
                Ok((
                    row.get::<_, TrackId>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )?;
        let mut tracks = Vec::new();

        for row in rows {
            let (id, path, title, album, duration_ms) = row?;
            let artists = self.artists_for_track(id)?;
            tracks.push(stored_track(id, path, title, album, duration_ms, artists)?);
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
                 modified_at INTEGER NOT NULL
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
        Ok(())
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

fn stored_track(
    id: TrackId,
    path: String,
    title: String,
    album: Option<String>,
    duration_ms: Option<i64>,
    artists: Vec<String>,
) -> Result<StoredTrack, StorageError> {
    let duration = duration_ms
        .map(u64::try_from)
        .transpose()
        .map_err(|_| StorageError::DurationOverflow)?
        .map(Duration::from_millis);
    Ok(StoredTrack {
        id,
        metadata: TrackMetadata {
            path: path.into(),
            title,
            album,
            artists,
            duration,
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

        assert_eq!(version, 2);
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
            .upsert_track(&track, 1)
            .expect("track should be inserted");
        track.title = "清音（更新）".into();
        let second_id = database
            .upsert_track(&track, 2)
            .expect("track should be updated");

        assert_eq!(first_id, second_id);
        assert_eq!(database.track_modified_at(&track.path).unwrap(), Some(2));
        let tracks = database.list_tracks().expect("tracks should be listed");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].metadata, track);
        assert!(database.remove_track(&track.path).unwrap());
        assert!(database.list_tracks().unwrap().is_empty());
    }

    #[test]
    fn searches_fields_separately_and_ranks_title_first() {
        let mut database = Database::open_in_memory().expect("in-memory database should open");
        let title_hit = TrackMetadata {
            path: "/music/qingyin.flac".into(),
            title: "清音".into(),
            album: Some("山水之间".into()),
            artists: vec!["测试歌手".into()],
            duration: Some(Duration::from_secs(42)),
        };
        let album_hit = TrackMetadata {
            path: "/music/other.flac".into(),
            title: "夜色".into(),
            album: Some("清音".into()),
            artists: Vec::new(),
            duration: None,
        };
        let latin_track = TrackMetadata {
            path: "/music/rain.flac".into(),
            title: "Rain".into(),
            album: Some("Wait".into()),
            artists: vec!["Love".into()],
            duration: None,
        };
        let special_track = TrackMetadata {
            path: "/music/breeze.flac".into(),
            title: "清风 100%_".into(),
            album: None,
            artists: Vec::new(),
            duration: None,
        };
        database.upsert_track(&title_hit, 1).unwrap();
        database.upsert_track(&album_hit, 1).unwrap();
        database.upsert_track(&latin_track, 1).unwrap();
        database.upsert_track(&special_track, 1).unwrap();

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
        assert_eq!(version, 2);
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
}
