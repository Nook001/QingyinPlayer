use std::path::{Path, PathBuf};
use std::time::Duration;

use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
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
    /// Optional tagged sort values (`TITLESORT` / `TSOT`, etc.). Empty means derive from display.
    pub title_sort: Option<String>,
    pub album_sort: Option<String>,
    pub artist_sort: Option<String>,
    /// Filesystem mtime in unix seconds when known; `0` if not yet recorded.
    pub modified_at: i64,
    /// Precomputed collation keys from display names and optional sort tags.
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
            modified_at: 0,
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

/// Reads tags, duration, and the first embedded picture from one Lofty parse.
///
/// # Errors
///
/// Returns [`MetadataError`] when the file cannot be opened or parsed by Lofty.
pub fn read_tagged_track(
    path: impl AsRef<Path>,
) -> Result<(TrackMetadata, Option<CoverArt>), MetadataError> {
    let path = path.as_ref();
    let tagged_file = lofty::read_from_path(path).map_err(|error| MetadataError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(parse_tagged_file(path, &tagged_file))
}

/// Reads common metadata and audio properties from a local audio file.
///
/// # Errors
///
/// Returns [`MetadataError`] when the file cannot be opened or parsed by Lofty.
pub fn read_track(path: impl AsRef<Path>) -> Result<TrackMetadata, MetadataError> {
    Ok(read_tagged_track(path)?.0)
}

/// Reads the first embedded picture from a local audio file.
///
/// # Errors
///
/// Returns [`MetadataError`] when the file cannot be opened or parsed by Lofty.
pub fn read_cover(path: impl AsRef<Path>) -> Result<Option<CoverArt>, MetadataError> {
    Ok(read_tagged_track(path)?.1)
}

fn parse_tagged_file(path: &Path, tagged_file: &TaggedFile) -> (TrackMetadata, Option<CoverArt>) {
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
        modified_at: 0,
        title_key: String::new(),
        album_key: String::new(),
        artist_key: String::new(),
    };
    track.refresh_sort_keys();
    (track, first_cover(tagged_file))
}

fn first_cover(tagged_file: &TaggedFile) -> Option<CoverArt> {
    tagged_file
        .tags()
        .iter()
        .find_map(|tag| tag.pictures().first())
        .map(|picture| CoverArt {
            data: picture.data().to_vec(),
            extension: picture
                .mime_type()
                .map_or("bin", cover_extension)
                .to_owned(),
        })
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
            modified_at: 0,
            title_key: String::new(),
            album_key: String::new(),
            artist_key: String::new(),
        };
        track.refresh_sort_keys();
        assert_eq!(track.title_key, "Jay Chou");
        assert_eq!(track.artist_key, "Jay Chou");
        assert_eq!(track.album_key.to_lowercase(), "yehuimei");
    }

    #[test]
    fn tagged_track_reads_metadata_and_cover_from_one_file() {
        let directory = std::env::temp_dir().join(format!(
            "qingyin-metadata-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("清音.wav");
        write_silence_wav(&path);

        let (track, cover) = read_tagged_track(&path).unwrap();
        assert_eq!(track.title, "清音");
        assert!(cover.is_none());
        assert_eq!(read_cover(&path).unwrap(), None);
        assert_eq!(read_track(&path).unwrap().title, "清音");

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn workspace_flac_has_embedded_cover_on_some_tag() {
        let path = Path::new("/mnt/workspace/Music/白鸟梦歌.flac");
        if !path.exists() {
            return;
        }
        let tagged = lofty::read_from_path(path).expect("should parse flac");
        let pictures: Vec<_> = tagged
            .tags()
            .iter()
            .map(|tag| {
                (
                    format!("{:?}", tag.tag_type()),
                    tag.pictures().len(),
                    tag.pictures().first().map(|picture| picture.data().len()),
                )
            })
            .collect();
        let cover = read_cover(path).expect("read_cover should succeed");
        assert!(
            cover.is_some(),
            "expected embedded cover; tags={pictures:?} primary={:?}",
            tagged.primary_tag().map(|tag| format!(
                "{:?} pics={}",
                tag.tag_type(),
                tag.pictures().len()
            ))
        );
    }

    fn write_silence_wav(path: &Path) {
        let data_len = 16u32;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + data_len).to_le_bytes());
        bytes.extend(b"WAVE");
        bytes.extend(b"fmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(8000u32.to_le_bytes());
        bytes.extend(16000u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(data_len.to_le_bytes());
        bytes.extend(vec![0_u8; data_len as usize]);
        std::fs::write(path, bytes).unwrap();
    }
}
