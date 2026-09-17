use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use qingyin_metadata::{MetadataError, TrackMetadata, read_track};
use qingyin_storage::{Database, StorageError};
use thiserror::Error;

const AUDIO_EXTENSIONS: &[&str] = &[
    "aac", "aif", "aiff", "ape", "flac", "m4a", "mp3", "mp4", "mpc", "ogg", "opus", "spx", "wav",
    "wv",
];

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanFailure {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanSummary {
    pub discovered: usize,
    pub imported: usize,
    pub unchanged: usize,
    pub failed: Vec<ScanFailure>,
}

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

    /// Recursively scans a directory and persists changed audio files.
    ///
    /// Individual metadata failures are returned in [`ScanSummary`].
    ///
    /// # Errors
    ///
    /// Returns [`LibraryError`] when the directory cannot be read or storage fails.
    pub fn scan_directory(
        &mut self,
        database: &mut Database,
        directory: impl AsRef<Path>,
    ) -> Result<ScanSummary, LibraryError> {
        let mut paths = Vec::new();
        collect_audio_paths(directory.as_ref(), &mut paths)?;
        paths.sort_unstable();

        let mut summary = ScanSummary {
            discovered: paths.len(),
            ..ScanSummary::default()
        };
        for path in paths {
            let modified_at = modified_at(&path)?;
            if database.track_modified_at(&path)? == Some(modified_at) {
                summary.unchanged += 1;
                continue;
            }

            match read_track(&path) {
                Ok(track) => {
                    database.upsert_track(&track, modified_at)?;
                    summary.imported += 1;
                }
                Err(error) => summary.failed.push(scan_failure(path, &error)),
            }
        }

        self.tracks = database
            .list_tracks()?
            .into_iter()
            .map(|track| track.metadata)
            .collect();
        Ok(summary)
    }
}

fn collect_audio_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), LibraryError> {
    let entries = fs::read_dir(directory).map_err(|source| LibraryError::Io {
        path: directory.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| LibraryError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| LibraryError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            collect_audio_paths(&path, paths)?;
        } else if file_type.is_file() && is_supported_audio(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            AUDIO_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

fn modified_at(path: &Path) -> Result<i64, LibraryError> {
    let metadata = fs::metadata(path).map_err(|source| LibraryError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let timestamp = metadata
        .modified()
        .map_err(|source| LibraryError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    Ok(i64::try_from(timestamp).unwrap_or(i64::MAX))
}

fn scan_failure(path: PathBuf, error: &MetadataError) -> ScanFailure {
    ScanFailure {
        path,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_extensions_case_insensitively() {
        assert!(is_supported_audio(Path::new("track.FLAC")));
        assert!(is_supported_audio(Path::new("track.mp3")));
        assert!(!is_supported_audio(Path::new("cover.jpg")));
        assert!(!is_supported_audio(Path::new("lyrics.lrc")));
    }
}
