use std::path::{Path, PathBuf};
use std::time::Duration;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::Accessor;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub path: PathBuf,
    pub title: String,
    pub album: Option<String>,
    pub artists: Vec<String>,
    pub duration: Option<Duration>,
}

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("failed to read metadata from {path}: {message}")]
    Read { path: PathBuf, message: String },
}

/// Reads common metadata and audio properties from a local audio file.
///
/// # Errors
///
/// Returns [`MetadataError`] when the file cannot be opened or parsed by Lofty.
pub fn read_track(path: impl AsRef<Path>) -> Result<TrackMetadata, MetadataError> {
    let path = path.as_ref();
    let tagged_file = lofty::read_from_path(path).map_err(|error| MetadataError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let fallback_title = path.file_stem().map_or_else(
        || path.display().to_string(),
        |value| value.to_string_lossy().into_owned(),
    );
    let title = tag
        .and_then(Accessor::title)
        .filter(|value| !value.trim().is_empty())
        .map_or(fallback_title, |value| value.trim().to_owned());
    let album = tag
        .and_then(Accessor::album)
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned());
    let artists = tag
        .and_then(Accessor::artist)
        .filter(|value| !value.trim().is_empty())
        .map_or_else(Vec::new, |value| vec![value.trim().to_owned()]);
    let duration = tagged_file.properties().duration();

    Ok(TrackMetadata {
        path: path.to_path_buf(),
        title,
        album,
        artists,
        duration: (!duration.is_zero()).then_some(duration),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_title_uses_the_file_stem() {
        let path = Path::new("/music/没有标题.flac");
        let fallback = path.file_stem().map_or_else(
            || path.display().to_string(),
            |value| value.to_string_lossy().into_owned(),
        );

        assert_eq!(fallback, "没有标题");
    }
}
