use std::path::{Path, PathBuf};
use std::time::Duration;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::MimeType;
use lofty::tag::{Accessor, ItemKey, Tag};
use qingyin_chinese::sort_key;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub path: PathBuf,
    pub title: String,
    pub album: Option<String>,
    pub artists: Vec<String>,
    pub duration: Option<Duration>,
    pub title_sort: Option<String>,
    pub album_sort: Option<String>,
    pub artist_sort: Option<String>,
    pub title_key: String,
    pub album_key: String,
    pub artist_key: String,
}

impl TrackMetadata {
    /// Builds metadata and computes collation keys from display names.
    #[must_use]
    pub fn from_display(
        path: impl Into<PathBuf>,
        title: impl Into<String>,
        album: Option<String>,
        artists: Vec<String>,
        duration: Option<Duration>,
    ) -> Self {
        let mut track = Self {
            path: path.into(),
            title: title.into(),
            album,
            artists,
            duration,
            title_sort: None,
            album_sort: None,
            artist_sort: None,
            title_key: String::new(),
            album_key: String::new(),
            artist_key: String::new(),
        };
        track.refresh_sort_keys();
        track
    }

    /// Rebuilds collation keys from display names and optional sort tags.
    pub fn refresh_sort_keys(&mut self) {
        self.title_key = sort_key(&self.title, self.title_sort.as_deref());
        self.album_key = sort_key(
            self.album.as_deref().unwrap_or(""),
            self.album_sort.as_deref(),
        );
        self.artist_key = sort_key(
            self.artists.first().map_or("", String::as_str),
            self.artist_sort.as_deref(),
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverArt {
    pub data: Vec<u8>,
    pub extension: String,
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
    let mut track = TrackMetadata {
        path: path.to_path_buf(),
        title,
        album,
        artists,
        duration: (!duration.is_zero()).then_some(duration),
        title_sort: sort_tag(tag, ItemKey::TrackTitleSortOrder),
        album_sort: sort_tag(tag, ItemKey::AlbumTitleSortOrder),
        artist_sort: sort_tag(tag, ItemKey::TrackArtistSortOrder),
        title_key: String::new(),
        album_key: String::new(),
        artist_key: String::new(),
    };
    track.refresh_sort_keys();
    Ok(track)
}

/// Reads the first embedded picture from a local audio file.
///
/// # Errors
///
/// Returns [`MetadataError`] when the file cannot be opened or parsed by Lofty.
pub fn read_cover(path: impl AsRef<Path>) -> Result<Option<CoverArt>, MetadataError> {
    let path = path.as_ref();
    let tagged_file = lofty::read_from_path(path).map_err(|error| MetadataError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let picture = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
        .and_then(|tag| tag.pictures().first());

    Ok(picture.map(|picture| CoverArt {
        data: picture.data().to_vec(),
        extension: picture
            .mime_type()
            .map_or("bin", cover_extension)
            .to_owned(),
    }))
}

fn sort_tag(tag: Option<&Tag>, key: ItemKey) -> Option<String> {
    tag.and_then(|tag| tag.get_string(key))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn cover_extension(mime_type: &MimeType) -> &'static str {
    match mime_type {
        MimeType::Jpeg => "jpg",
        MimeType::Png => "png",
        MimeType::Tiff => "tiff",
        MimeType::Bmp => "bmp",
        MimeType::Gif => "gif",
        _ => "bin",
    }
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

    #[test]
    fn refresh_sort_keys_uses_tags_and_pinyin() {
        let mut track = TrackMetadata {
            path: "/music/a.flac".into(),
            title: "周杰伦".into(),
            album: Some("叶惠美".into()),
            artists: vec!["周杰伦".into()],
            duration: None,
            title_sort: Some("Jay Chou".into()),
            album_sort: None,
            artist_sort: Some("Jay Chou".into()),
            title_key: String::new(),
            album_key: String::new(),
            artist_key: String::new(),
        };
        track.refresh_sort_keys();
        assert_eq!(track.title_key, "Jay Chou");
        assert_eq!(track.artist_key, "Jay Chou");
        assert_eq!(track.album_key.to_lowercase(), "yehuimei");
    }
}
