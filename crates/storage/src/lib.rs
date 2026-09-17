use std::path::Path;

use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_the_initial_schema() {
        let database = Database::open_in_memory().expect("in-memory database should open");
        let version = database
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .expect("schema version should be readable");

        assert_eq!(version, 1);
    }
}
