use std::path::{Path, PathBuf};
use std::time::Duration;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::picture::{MimeType, PictureType};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub path: PathBuf,
    pub id: i64,
    pub title: String,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub artists: Vec<String>,
    pub duration: Option<Duration>,
    pub disc_number: Option<u32>,
    pub track_number: Option<u32>,
    /// Optional tagged sort values (`TITLESORT` / `TSOT`, etc.). Empty means derive from display.
    pub title_sort: Option<String>,
    pub album_sort: Option<String>,
    pub artist_sort: Option<String>,
    pub file_size: u64,
    /// Filesystem mtime in unix nanoseconds when known; `0` if not yet recorded.
    pub modified_at_ns: i64,
    /// SHA-256 of the selected embedded picture, when one was stored.
    pub cover_digest: Option<String>,
}

impl TrackMetadata {
    /// Builds tag fields from display names. Collation keys are computed later
    /// when the track is loaded into a UI snapshot.
    #[must_use]
    pub fn from_display(
        path: impl Into<PathBuf>,
        title: impl Into<String>,
        album: Option<String>,
        artists: Vec<String>,
        duration: Option<Duration>,
    ) -> Self {
        Self {
            path: path.into(),
            id: 0,
            title: title.into(),
            album,
            album_artist: None,
            artists,
            duration,
            disc_number: None,
            track_number: None,
            title_sort: None,
            album_sort: None,
            artist_sort: None,
            file_size: 0,
            modified_at_ns: 0,
            cover_digest: None,
        }
    }

    #[must_use]
    pub fn fingerprint(&self) -> FileFingerprint {
        FileFingerprint {
            modified_at_ns: self.modified_at_ns,
            file_size: self.file_size,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileFingerprint {
    pub modified_at_ns: i64,
    pub file_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverArt {
    pub data: Vec<u8>,
    pub extension: String,
}

impl CoverArt {
    #[must_use]
    pub fn digest(&self) -> String {
        use sha2::{Digest, Sha256};
        hex_digest(Sha256::digest(&self.data).as_slice())
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("failed to read metadata from {path}: {message}")]
    Read { path: PathBuf, message: String },
}

/// Reads tags, duration, and the preferred embedded picture from one Lofty parse.
///
/// # Errors
///
/// Returns [`MetadataError`] when the file cannot be opened or parsed by Lofty.
pub fn read_tagged_track(
    path: impl AsRef<Path>,
) -> Result<(TrackMetadata, Option<CoverArt>), MetadataError> {
    let path = path.as_ref();
    let tagged_file = probe_path(
        path,
        ParseOptions::new()
            .read_properties(true)
            .read_cover_art(true),
    )?;
    Ok(parse_tagged_file(path, &tagged_file, true))
}

/// Reads common metadata and audio properties without copying embedded pictures.
///
/// # Errors
///
/// Returns [`MetadataError`] when the file cannot be opened or parsed by Lofty.
pub fn read_track(path: impl AsRef<Path>) -> Result<TrackMetadata, MetadataError> {
    let path = path.as_ref();
    let tagged_file = probe_path(
        path,
        ParseOptions::new()
            .read_properties(true)
            .read_cover_art(false),
    )?;
    Ok(parse_tagged_file(path, &tagged_file, false).0)
}

/// Reads the preferred embedded picture, skipping unrelated property parsing.
///
/// # Errors
///
/// Returns [`MetadataError`] when the file cannot be opened or parsed by Lofty.
pub fn read_cover(path: impl AsRef<Path>) -> Result<Option<CoverArt>, MetadataError> {
    let path = path.as_ref();
    let tagged_file = probe_path(
        path,
        ParseOptions::new()
            .read_properties(false)
            .read_cover_art(true),
    )?;
    Ok(preferred_cover(&tagged_file))
}

fn probe_path(path: &Path, options: ParseOptions) -> Result<TaggedFile, MetadataError> {
    Probe::open(path)
        .and_then(|probe| probe.options(options).read())
        .map_err(|error| MetadataError::Read {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
}

fn parse_tagged_file(
    path: &Path,
    tagged_file: &TaggedFile,
    include_cover: bool,
) -> (TrackMetadata, Option<CoverArt>) {
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
    let artists = tag.map(read_artists).unwrap_or_default();
    let duration = tagged_file.properties().duration();
    let mut track = TrackMetadata {
        path: path.to_path_buf(),
        id: 0,
        title,
        album,
        album_artist: text_tag(tag, ItemKey::AlbumArtist)
            .or_else(|| text_tag(tag, ItemKey::AlbumArtists)),
        artists,
        duration: (!duration.is_zero()).then_some(duration),
        disc_number: number_tag(tag, ItemKey::DiscNumber),
        track_number: number_tag(tag, ItemKey::TrackNumber),
        title_sort: sort_tag(tag, ItemKey::TrackTitleSortOrder),
        album_sort: sort_tag(tag, ItemKey::AlbumTitleSortOrder),
        artist_sort: sort_tag(tag, ItemKey::TrackArtistSortOrder),
        file_size: 0,
        modified_at_ns: 0,
        cover_digest: None,
    };
    let cover = if include_cover {
        preferred_cover(tagged_file)
    } else {
        None
    };
    if let Some(cover) = &cover {
        track.cover_digest = Some(cover.digest());
    }
    (track, cover)
}

fn read_artists(tag: &Tag) -> Vec<String> {
    let mut artists = unique_values(tag.get_strings(ItemKey::TrackArtists));
    if artists.is_empty() {
        artists = unique_values(tag.get_strings(ItemKey::TrackArtist));
    }
    if artists.is_empty() {
        artists = tag
            .artist()
            .filter(|value| !value.trim().is_empty())
            .map_or_else(Vec::new, |value| vec![value.trim().to_owned()]);
    }
    artists
}

fn unique_values<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut artists = Vec::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() || !seen.insert(value.to_owned()) {
            continue;
        }
        artists.push(value.to_owned());
    }
    artists
}

fn preferred_cover(tagged_file: &TaggedFile) -> Option<CoverArt> {
    tagged_file.tags().iter().find_map(|tag| {
        tag.pictures()
            .iter()
            .find(|picture| picture.pic_type() == PictureType::CoverFront)
            .or_else(|| {
                tag.pictures()
                    .iter()
                    .find(|picture| picture.pic_type() == PictureType::Other)
            })
            .or_else(|| tag.pictures().first())
            .map(|picture| CoverArt {
                data: picture.data().to_vec(),
                extension: picture
                    .mime_type()
                    .map_or("bin", cover_extension)
                    .to_owned(),
            })
    })
}

fn text_tag(tag: Option<&Tag>, key: ItemKey) -> Option<String> {
    tag.and_then(|tag| tag.get_string(key))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn number_tag(tag: Option<&Tag>, key: ItemKey) -> Option<u32> {
    text_tag(tag, key).and_then(|value| {
        value
            .split(['/', '-'])
            .next()
            .and_then(|number| number.trim().parse().ok())
    })
}

fn sort_tag(tag: Option<&Tag>, key: ItemKey) -> Option<String> {
    text_tag(tag, key)
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
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::tag::{ItemValue, TagItem, TagType};

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
    fn from_display_keeps_sort_tags_empty() {
        let track = TrackMetadata::from_display(
            "/music/a.flac",
            "周杰伦",
            Some("叶惠美".into()),
            vec!["周杰伦".into()],
            None,
        );
        assert_eq!(track.title, "周杰伦");
        assert!(track.title_sort.is_none());
        assert!(track.album_sort.is_none());
        assert!(track.artist_sort.is_none());
        assert!(track.album_artist.is_none());
        assert!(track.disc_number.is_none());
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
        crate::test_support::write_silence_wav(&path);

        let (track, cover) = read_tagged_track(&path).unwrap();
        assert_eq!(track.title, "清音");
        assert!(cover.is_none());
        assert_eq!(read_cover(&path).unwrap(), None);
        assert_eq!(read_track(&path).unwrap().title, "清音");

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn reads_ordered_artists_without_splitting_slashes() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.push_unchecked(TagItem::new(
            ItemKey::TrackArtists,
            ItemValue::Text("A / B".into()),
        ));
        tag.push_unchecked(TagItem::new(
            ItemKey::TrackArtists,
            ItemValue::Text("C".into()),
        ));
        assert_eq!(read_artists(&tag), vec!["A / B".to_owned(), "C".to_owned()]);
    }

    #[test]
    fn prefers_front_cover_over_later_pictures() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.push_picture(
            Picture::unchecked(b"other".to_vec())
                .pic_type(PictureType::Other)
                .mime_type(MimeType::Png)
                .build(),
        );
        tag.push_picture(
            Picture::unchecked(b"front".to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Jpeg)
                .build(),
        );
        let tagged = TaggedFile::new(
            lofty::file::FileType::Mpeg,
            lofty::properties::FileProperties::default(),
            vec![tag],
        );
        let cover = preferred_cover(&tagged).expect("front cover");
        assert_eq!(cover.extension, "jpg");
        assert_eq!(cover.data, b"front");
    }
}

pub mod test_support {
    use std::path::Path;

    #[must_use]
    pub fn silence_wav_bytes() -> Vec<u8> {
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
        bytes
    }

    pub fn write_silence_wav(path: &Path) {
        std::fs::write(path, silence_wav_bytes()).unwrap();
    }
}
