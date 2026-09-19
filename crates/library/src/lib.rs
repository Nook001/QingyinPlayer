mod watch;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use qingyin_chinese::{compare_keys, sort_key};
use qingyin_metadata::{CoverArt, TrackMetadata, read_tagged_track};
use qingyin_storage::{Database, StorageError};
use thiserror::Error;

pub use watch::{LibraryWatcher, WatchSummary, watch_directories};

pub const UNKNOWN_ARTIST: &str = "未知歌手";
pub const UNKNOWN_ALBUM: &str = "未知专辑";
const UPSERT_BATCH_SIZE: usize = 50;

const AUDIO_EXTENSIONS: &[&str] = &[
    "aac", "aif", "aiff", "ape", "flac", "m4a", "mp3", "mp4", "mpc", "ogg", "opus", "spx", "wav",
    "wv",
];

const SKIPPED_DIRECTORIES: &[&str] = &[
    "steamapps",
    "compatdata",
    "pfx",
    "drive_c",
    "dosdevices",
    ".venv",
    "venv",
    "site-packages",
    "node_modules",
    ".git",
    "__pycache__",
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
    Notify(#[from] notify::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryChange {
    Upserted(usize),
    Removed(usize),
    Ignored,
    Failed { path: PathBuf, message: String },
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

/// One file observed while scanning a music directory.
#[derive(Debug)]
pub enum ScanEvent<'a> {
    Imported {
        track: &'a TrackMetadata,
        modified_at: i64,
        cover: Option<CoverArt>,
    },
    Unchanged {
        path: &'a Path,
        modified_at: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionEntry {
    pub name: String,
    pub subtitle: String,
    pub cover_url: String,
    pub track_count: usize,
    pub tracks: Vec<Arc<TrackMetadata>>,
    pub cover_urls: Vec<String>,
}

#[derive(Debug, Default)]
pub struct MusicLibrary {
    tracks: Vec<TrackMetadata>,
}

/// Groups tracks under each listed artist. Tracks without artists use [`UNKNOWN_ARTIST`].
#[must_use]
pub fn aggregate_artists(tracks: &[TrackMetadata], cover_urls: &[String]) -> Vec<CollectionEntry> {
    let mut groups = HashMap::<String, Vec<(Arc<TrackMetadata>, String)>>::new();
    for (index, track) in tracks.iter().enumerate() {
        let cover = cover_url_at(cover_urls, index);
        let shared = Arc::new(track.clone());
        if track.artists.is_empty() {
            push_grouped_track(&mut groups, UNKNOWN_ARTIST.to_owned(), shared, cover);
            continue;
        }
        for name in &track.artists {
            push_grouped_track(
                &mut groups,
                name.clone(),
                Arc::clone(&shared),
                cover.clone(),
            );
        }
    }
    finish_collections(groups, CollectionKind::Artist)
}

/// Groups tracks by album title. Tracks without an album use [`UNKNOWN_ALBUM`].
#[must_use]
pub fn aggregate_albums(tracks: &[TrackMetadata], cover_urls: &[String]) -> Vec<CollectionEntry> {
    let mut groups = HashMap::<String, Vec<(Arc<TrackMetadata>, String)>>::new();
    for (index, track) in tracks.iter().enumerate() {
        let name = track
            .album
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(UNKNOWN_ALBUM)
            .to_owned();
        push_grouped_track(
            &mut groups,
            name,
            Arc::new(track.clone()),
            cover_url_at(cover_urls, index),
        );
    }
    finish_collections(groups, CollectionKind::Album)
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
    /// Individual metadata or storage failures are returned in [`ScanSummary`].
    ///
    /// # Errors
    ///
    /// Returns [`LibraryError`] when the directory cannot be read.
    pub fn scan_directory(
        &mut self,
        database: &mut Database,
        directory: impl AsRef<Path>,
    ) -> Result<ScanSummary, LibraryError> {
        let _ = self;
        scan_music_directory(database, directory, |_| {})
    }

    /// Scans a directory and reports each imported or unchanged file to `on_event`.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryError`] when the directory cannot be read.
    pub fn scan_directory_with(
        &mut self,
        database: &mut Database,
        directory: impl AsRef<Path>,
        on_event: impl FnMut(ScanEvent<'_>),
    ) -> Result<ScanSummary, LibraryError> {
        let _ = self;
        scan_music_directory(database, directory, on_event)
    }

    /// Applies a single filesystem change under the watched music directories.
    ///
    /// Missing files and directories remove matching library rows. Existing audio is
    /// imported when its modification time changed. Temporary files, non-audio files,
    /// and paths outside `watched_roots` are ignored. This does not reload the in-memory
    /// track list; call [`Self::sync_from_database`] when that snapshot is needed.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryError`] when the path cannot be read or storage fails.
    pub fn refresh_path(
        &mut self,
        database: &mut Database,
        path: impl AsRef<Path>,
        watched_roots: &[PathBuf],
    ) -> Result<LibraryChange, LibraryError> {
        let _ = self;
        refresh_watched_path(database, path, watched_roots)
    }

    /// Reloads the in-memory track list from SQLite.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryError`] when storage cannot list tracks.
    pub fn sync_from_database(&mut self, database: &Database) -> Result<(), LibraryError> {
        self.sync_tracks(database)
    }

    fn sync_tracks(&mut self, database: &Database) -> Result<(), LibraryError> {
        self.tracks = database
            .list_tracks()?
            .into_iter()
            .map(|track| track.metadata)
            .collect();
        Ok(())
    }
}

/// Applies a watched filesystem change without loading the full library into memory.
///
/// # Errors
///
/// Returns [`LibraryError`] when the path cannot be read or storage fails.
pub fn refresh_watched_path(
    database: &mut Database,
    path: impl AsRef<Path>,
    watched_roots: &[PathBuf],
) -> Result<LibraryChange, LibraryError> {
    let path = path.as_ref();
    if !is_library_track(path, watched_roots) || is_temporary(path) {
        return Ok(LibraryChange::Ignored);
    }

    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => refresh_directory(database, path),
        Ok(_) if is_supported_audio(path) => refresh_audio_file(database, path),
        Ok(_) => {
            if database.remove_track(path)? {
                Ok(LibraryChange::Removed(1))
            } else {
                Ok(LibraryChange::Ignored)
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => refresh_missing_path(database, path),
        Err(error) => Err(LibraryError::Io {
            path: path.to_path_buf(),
            source: error,
        }),
    }
}

fn scan_music_directory(
    database: &mut Database,
    directory: impl AsRef<Path>,
    mut on_event: impl FnMut(ScanEvent<'_>),
) -> Result<ScanSummary, LibraryError> {
    let mut paths = Vec::new();
    collect_audio_paths(directory.as_ref(), &mut paths)?;
    paths.sort_unstable();
    import_audio_files(database, &paths, &mut on_event)
}

fn import_audio_files(
    database: &mut Database,
    paths: &[PathBuf],
    on_event: &mut impl FnMut(ScanEvent<'_>),
) -> Result<ScanSummary, LibraryError> {
    let mut summary = ScanSummary {
        discovered: paths.len(),
        ..ScanSummary::default()
    };
    let mut pending = Vec::new();
    for path in paths {
        let modified_at = modified_at(path)?;
        if database.track_modified_at(path)? == Some(modified_at) {
            on_event(ScanEvent::Unchanged { path, modified_at });
            summary.unchanged += 1;
            continue;
        }
        match read_tagged_track(path) {
            Ok((mut track, cover)) => {
                track.modified_at = modified_at;
                pending.push((track, modified_at, cover));
                if pending.len() >= UPSERT_BATCH_SIZE {
                    flush_pending(database, &mut pending, on_event, &mut summary);
                }
            }
            Err(error) => summary
                .failed
                .push(scan_failure(path.clone(), error.to_string())),
        }
    }
    flush_pending(database, &mut pending, on_event, &mut summary);
    Ok(summary)
}

fn flush_pending(
    database: &mut Database,
    pending: &mut Vec<(TrackMetadata, i64, Option<CoverArt>)>,
    on_event: &mut impl FnMut(ScanEvent<'_>),
    summary: &mut ScanSummary,
) {
    if pending.is_empty() {
        return;
    }
    let batch = pending
        .iter()
        .map(|(track, modified_at, _)| (track, *modified_at))
        .collect::<Vec<_>>();
    if database.upsert_tracks(&batch).is_ok() {
        for (track, modified_at, cover) in pending.drain(..) {
            on_event(ScanEvent::Imported {
                track: &track,
                modified_at,
                cover,
            });
            summary.imported += 1;
        }
        return;
    }
    for (track, modified_at, cover) in pending.drain(..) {
        match database.upsert_track(&track, modified_at) {
            Ok(_) => {
                on_event(ScanEvent::Imported {
                    track: &track,
                    modified_at,
                    cover,
                });
                summary.imported += 1;
            }
            Err(error) => summary
                .failed
                .push(scan_failure(track.path.clone(), error.to_string())),
        }
    }
}

fn refresh_directory(
    database: &mut Database,
    directory: &Path,
) -> Result<LibraryChange, LibraryError> {
    match scan_music_directory(database, directory, |_| {}) {
        Ok(summary) if summary.imported > 0 => Ok(LibraryChange::Upserted(summary.imported)),
        Ok(summary) if !summary.failed.is_empty() && summary.unchanged == 0 => {
            Ok(LibraryChange::Failed {
                path: directory.to_path_buf(),
                message: summary.failed[0].message.clone(),
            })
        }
        Ok(_) => Ok(LibraryChange::Ignored),
        Err(LibraryError::Io { .. }) => remove_missing_prefix(database, directory),
        Err(error) => Err(error),
    }
}

fn refresh_audio_file(database: &mut Database, path: &Path) -> Result<LibraryChange, LibraryError> {
    let modified_at = modified_at(path)?;
    if database.track_modified_at(path)? == Some(modified_at) {
        return Ok(LibraryChange::Ignored);
    }
    match read_tagged_track(path) {
        Ok((mut track, _cover)) => {
            track.modified_at = modified_at;
            database.upsert_track(&track, modified_at)?;
            Ok(LibraryChange::Upserted(1))
        }
        Err(error) => Ok(LibraryChange::Failed {
            path: path.to_path_buf(),
            message: error.to_string(),
        }),
    }
}

fn refresh_missing_path(
    database: &mut Database,
    path: &Path,
) -> Result<LibraryChange, LibraryError> {
    if is_supported_audio(path) {
        if database.remove_track(path)? {
            return Ok(LibraryChange::Removed(1));
        }
        return Ok(LibraryChange::Ignored);
    }
    remove_missing_prefix(database, path)
}

fn remove_missing_prefix(
    database: &mut Database,
    path: &Path,
) -> Result<LibraryChange, LibraryError> {
    let removed = database.remove_tracks_under(path)?;
    if removed == 0 {
        return Ok(LibraryChange::Ignored);
    }
    Ok(LibraryChange::Removed(removed))
}

fn collect_audio_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), LibraryError> {
    collect_audio_paths_inner(directory, paths, true)
}

fn collect_audio_paths_inner(
    directory: &Path,
    paths: &mut Vec<PathBuf>,
    is_root: bool,
) -> Result<(), LibraryError> {
    if !is_root && is_skipped_scan_directory(directory) {
        return Ok(());
    }

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
            collect_audio_paths_inner(&path, paths, false)?;
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

pub(crate) fn is_temporary(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("part")
                || extension.eq_ignore_ascii_case("tmp")
                || extension.eq_ignore_ascii_case("temp")
        })
}

/// Returns whether `path` is under a configured music root and not inside a skipped tree.
#[must_use]
pub fn is_library_track(path: &Path, roots: &[PathBuf]) -> bool {
    is_watched_path(path, roots) && !is_excluded_library_path(path, roots)
}

/// Deletes stored tracks that are outside `roots` or sit in skipped vendor/runtime folders.
///
/// # Errors
///
/// Returns [`LibraryError`] when storage cannot list or delete tracks.
pub fn prune_unwatched_tracks(
    database: &Database,
    roots: &[PathBuf],
) -> Result<usize, LibraryError> {
    let mut removed = 0;
    for track in database.list_tracks()? {
        if is_library_track(&track.metadata.path, roots) {
            continue;
        }
        if database.remove_track(&track.metadata.path)? {
            removed += 1;
        }
    }
    Ok(removed)
}

pub(crate) fn is_watched_path(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| path == root || path.starts_with(root))
}

fn is_excluded_library_path(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        if path == root {
            return false;
        }
        path.strip_prefix(root).is_ok_and(|relative| {
            relative.components().any(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .is_some_and(is_skipped_directory_name)
            })
        })
    })
}

fn is_skipped_scan_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_skipped_directory_name)
}

fn is_skipped_directory_name(name: &str) -> bool {
    SKIPPED_DIRECTORIES
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
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

fn scan_failure(path: PathBuf, message: String) -> ScanFailure {
    ScanFailure { path, message }
}

#[derive(Clone, Copy)]
enum CollectionKind {
    Artist,
    Album,
}

fn cover_url_at(cover_urls: &[String], index: usize) -> String {
    cover_urls.get(index).cloned().unwrap_or_default()
}

fn push_grouped_track(
    groups: &mut HashMap<String, Vec<(Arc<TrackMetadata>, String)>>,
    name: String,
    track: Arc<TrackMetadata>,
    cover: String,
) {
    groups.entry(name).or_default().push((track, cover));
}

fn finish_collections(
    groups: HashMap<String, Vec<(Arc<TrackMetadata>, String)>>,
    kind: CollectionKind,
) -> Vec<CollectionEntry> {
    let mut groups = groups
        .into_iter()
        .map(|(name, tracks)| {
            let key = collection_name_key(&name, &tracks, kind);
            (name, key, tracks)
        })
        .collect::<Vec<_>>();
    groups.sort_by(|(left_name, left_key, _), (right_name, right_key, _)| {
        compare_collection_name(left_name, left_key, right_name, right_key)
    });
    groups
        .into_iter()
        .map(|(name, _, mut tracks)| {
            tracks.sort_by(|(left, _), (right, _)| compare_grouped_tracks(left, right, kind));
            let cover_url = tracks
                .iter()
                .find_map(|(_, cover)| (!cover.is_empty()).then(|| cover.clone()))
                .unwrap_or_default();
            let subtitle = collection_subtitle(&name, &tracks, kind);
            let track_count = tracks.len();
            let (tracks, cover_urls) = tracks.into_iter().unzip();
            CollectionEntry {
                name,
                subtitle,
                cover_url,
                track_count,
                tracks,
                cover_urls,
            }
        })
        .collect()
}

fn collection_subtitle(
    name: &str,
    tracks: &[(Arc<TrackMetadata>, String)],
    kind: CollectionKind,
) -> String {
    match kind {
        CollectionKind::Artist => format_count(tracks.len(), "首歌曲"),
        CollectionKind::Album => {
            let artists = unique_artists(tracks);
            if artists.is_empty() {
                format!("{name} · {}", format_count(tracks.len(), "首歌曲"))
            } else {
                format!(
                    "{} · {}",
                    artists.join("、"),
                    format_count(tracks.len(), "首歌曲")
                )
            }
        }
    }
}

fn unique_artists(tracks: &[(Arc<TrackMetadata>, String)]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut artists = Vec::new();
    for (track, _) in tracks {
        for artist in &track.artists {
            if artist.is_empty() || !seen.insert(artist.as_str()) {
                continue;
            }
            artists.push((artist.clone(), sort_key(artist, None)));
        }
    }
    artists.sort_by(|left, right| compare_keys(&left.1, &right.1));
    artists.into_iter().map(|(name, _)| name).collect()
}

fn format_count(count: usize, unit: &str) -> String {
    format!("{count} {unit}")
}

fn compare_collection_name(left: &str, left_key: &str, right: &str, right_key: &str) -> Ordering {
    match (is_unknown_collection(left), is_unknown_collection(right)) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => compare_keys(left_key, right_key),
    }
}

fn collection_name_key(
    name: &str,
    tracks: &[(Arc<TrackMetadata>, String)],
    kind: CollectionKind,
) -> String {
    sort_key(name, collection_sort_tag(name, tracks, kind))
}

fn collection_sort_tag<'a>(
    name: &str,
    tracks: &'a [(Arc<TrackMetadata>, String)],
    kind: CollectionKind,
) -> Option<&'a str> {
    tracks.iter().find_map(|(track, _)| match kind {
        CollectionKind::Artist
            if track.artists.len() == 1
                && track.artists.first().map(String::as_str) == Some(name) =>
        {
            nonempty_sort_tag(track.artist_sort.as_deref())
        }
        CollectionKind::Album if track.album.as_deref() == Some(name) => {
            nonempty_sort_tag(track.album_sort.as_deref())
        }
        _ => None,
    })
}

fn nonempty_sort_tag(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn is_unknown_collection(name: &str) -> bool {
    name == UNKNOWN_ARTIST || name == UNKNOWN_ALBUM
}

fn compare_grouped_tracks(
    left: &TrackMetadata,
    right: &TrackMetadata,
    kind: CollectionKind,
) -> Ordering {
    match kind {
        CollectionKind::Artist => compare_keys(&left.album_key, &right.album_key)
            .then_with(|| compare_keys(&left.title_key, &right.title_key))
            .then_with(|| left.path.cmp(&right.path)),
        CollectionKind::Album => {
            compare_keys(&left.title_key, &right.title_key).then_with(|| left.path.cmp(&right.path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn recognizes_supported_extensions_case_insensitively() {
        assert!(is_supported_audio(Path::new("track.FLAC")));
        assert!(is_supported_audio(Path::new("track.mp3")));
        assert!(!is_supported_audio(Path::new("cover.jpg")));
        assert!(!is_supported_audio(Path::new("lyrics.lrc")));
    }

    #[test]
    fn aggregates_artists_and_albums_from_library_tracks() {
        let tracks = vec![
            test_track(
                "夜色",
                Some("清音"),
                vec!["乙歌手", "甲歌手"],
                "/music/night.flac",
            ),
            test_track("晨光", Some("清音"), vec!["甲歌手"], "/music/morning.flac"),
            test_track("无题", None, Vec::new(), "/music/untitled.flac"),
        ];
        let covers = vec![
            "night-cover".to_owned(),
            "morning-cover".to_owned(),
            String::new(),
        ];

        let artists = aggregate_artists(&tracks, &covers);
        assert_eq!(artists.len(), 3);
        assert_eq!(artists[0].name, "甲歌手");
        assert_eq!(artists[0].track_count, 2);
        assert_eq!(artists[0].cover_url, "morning-cover");
        assert_eq!(artists[0].tracks[0].title, "晨光");
        assert_eq!(artists[0].tracks[1].title, "夜色");
        assert_eq!(artists[1].name, "乙歌手");
        assert_eq!(artists[2].name, UNKNOWN_ARTIST);
        assert_eq!(artists[2].track_count, 1);

        let albums = aggregate_albums(&tracks, &covers);
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].name, "清音");
        assert_eq!(albums[0].track_count, 2);
        assert_eq!(albums[0].subtitle, "甲歌手、乙歌手 · 2 首歌曲");
        assert_eq!(albums[0].cover_url, "morning-cover");
        assert_eq!(albums[0].tracks[0].title, "晨光");
        assert_eq!(albums[0].tracks[1].title, "夜色");
        assert_eq!(albums[1].name, UNKNOWN_ALBUM);
        assert_eq!(albums[1].track_count, 1);
    }

    #[test]
    fn artist_sort_tags_reorder_collection_names() {
        let mut jay = test_track("晴天", Some("叶惠美"), vec!["周杰伦"], "/music/jay.flac");
        jay.artist_sort = Some("Aaa".into());
        jay.refresh_sort_keys();
        let li = test_track("麻雀", Some("麻雀"), vec!["李荣浩"], "/music/li.flac");
        let artists = aggregate_artists(&[jay, li], &["a".into(), "b".into()]);
        assert_eq!(artists[0].name, "周杰伦");
        assert_eq!(artists[1].name, "李荣浩");
    }

    #[test]
    fn artist_collections_interleave_han_and_latin_names() {
        let tracks = vec![
            test_track("晴天", None, vec!["周杰伦"], "/music/jay.flac"),
            test_track("Hello", None, vec!["Adele"], "/music/adele.flac"),
            test_track("听海", None, vec!["阿妹"], "/music/amei.flac"),
        ];
        let covers = vec![String::new(); 3];
        let artists = aggregate_artists(&tracks, &covers);
        assert_eq!(
            artists
                .iter()
                .map(|artist| artist.name.as_str())
                .collect::<Vec<_>>(),
            ["Adele", "阿妹", "周杰伦"]
        );
    }

    #[test]
    fn multi_artist_tracks_share_one_metadata_allocation() {
        let tracks = vec![test_track(
            "夜色",
            Some("清音"),
            vec!["乙歌手", "甲歌手"],
            "/music/night.flac",
        )];
        let covers = vec!["night-cover".to_owned()];
        let artists = aggregate_artists(&tracks, &covers);
        assert_eq!(artists.len(), 2);
        assert!(Arc::ptr_eq(&artists[0].tracks[0], &artists[1].tracks[0]));
    }

    #[test]
    fn refresh_path_ignores_temporary_unwatched_and_non_audio_files() {
        let mut database = Database::open_in_memory().unwrap();
        let mut library = MusicLibrary::default();
        let root = temp_music_dir();
        let roots = [root.clone()];
        let outside = std::env::temp_dir().join("qingyin-outside.flac");
        let image = root.join("cover.jpg");
        let partial = root.join("track.flac.part");
        fs::write(&image, b"not-audio").unwrap();
        fs::write(&partial, b"partial").unwrap();

        assert_eq!(
            library
                .refresh_path(&mut database, &outside, &roots)
                .unwrap(),
            LibraryChange::Ignored
        );
        assert_eq!(
            library
                .refresh_path(&mut database, &partial, &roots)
                .unwrap(),
            LibraryChange::Ignored
        );
        assert_eq!(
            library.refresh_path(&mut database, &image, &roots).unwrap(),
            LibraryChange::Ignored
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refresh_path_imports_updates_and_removes_audio_files() {
        let mut database = Database::open_in_memory().unwrap();
        let mut library = MusicLibrary::default();
        let root = temp_music_dir();
        let roots = [root.clone()];
        let path = root.join("清音.wav");
        write_silence_wav(&path);

        assert_eq!(
            library.refresh_path(&mut database, &path, &roots).unwrap(),
            LibraryChange::Upserted(1)
        );
        assert!(library.tracks().is_empty());
        library.sync_from_database(&database).unwrap();
        assert_eq!(library.tracks().len(), 1);
        assert_eq!(
            library.refresh_path(&mut database, &path, &roots).unwrap(),
            LibraryChange::Ignored
        );

        bump_mtime(&path);
        assert_eq!(
            library.refresh_path(&mut database, &path, &roots).unwrap(),
            LibraryChange::Upserted(1)
        );

        fs::remove_file(&path).unwrap();
        assert_eq!(
            library.refresh_path(&mut database, &path, &roots).unwrap(),
            LibraryChange::Removed(1)
        );
        library.sync_from_database(&database).unwrap();
        assert!(library.tracks().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refresh_path_removes_tracks_when_a_directory_disappears() {
        let mut database = Database::open_in_memory().unwrap();
        let mut library = MusicLibrary::default();
        let root = temp_music_dir();
        let album = root.join("album");
        fs::create_dir_all(&album).unwrap();
        let nested = album.join("track.wav");
        write_silence_wav(&nested);
        let roots = [root.clone()];

        assert_eq!(
            library.refresh_path(&mut database, &album, &roots).unwrap(),
            LibraryChange::Upserted(1)
        );

        fs::remove_dir_all(&album).unwrap();
        assert_eq!(
            library.refresh_path(&mut database, &album, &roots).unwrap(),
            LibraryChange::Removed(1)
        );
        library.sync_from_database(&database).unwrap();
        assert!(library.tracks().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_skips_proton_and_virtualenv_directories() {
        let mut database = Database::open_in_memory().unwrap();
        let mut library = MusicLibrary::default();
        let root = temp_music_dir();
        write_silence_wav(&root.join("keep.wav"));

        let proton = root.join("steamapps/compatdata/1/pfx");
        fs::create_dir_all(&proton).unwrap();
        write_silence_wav(&proton.join("junk.wav"));

        let venv = root.join(".venv/lib/site-packages");
        fs::create_dir_all(&venv).unwrap();
        write_silence_wav(&venv.join("also.wav"));

        library.scan_directory(&mut database, &root).unwrap();
        library.sync_from_database(&database).unwrap();
        assert_eq!(library.tracks().len(), 1);
        assert_eq!(library.tracks()[0].path.file_name().unwrap(), "keep.wav");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refresh_path_ignores_audio_under_skipped_directories() {
        let mut database = Database::open_in_memory().unwrap();
        let mut library = MusicLibrary::default();
        let root = temp_music_dir();
        let proton = root.join("steamapps/compatdata/1/pfx");
        fs::create_dir_all(&proton).unwrap();
        let junk = proton.join("junk.wav");
        write_silence_wav(&junk);
        let roots = [root.clone()];

        assert_eq!(
            library.refresh_path(&mut database, &junk, &roots).unwrap(),
            LibraryChange::Ignored
        );
        library.sync_from_database(&database).unwrap();
        assert!(library.tracks().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_removes_tracks_outside_roots_and_skipped_trees() {
        let mut database = Database::open_in_memory().unwrap();
        let root = temp_music_dir();
        let keep = root.join("keep.wav");
        write_silence_wav(&keep);
        let mut library = MusicLibrary::default();
        library.scan_directory(&mut database, &root).unwrap();

        let outside = std::env::temp_dir().join(format!(
            "qingyin-outside-{}-{}.wav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        write_silence_wav(&outside);
        database
            .upsert_track(
                &test_track("outside", None, Vec::new(), outside.to_str().unwrap()),
                1,
            )
            .unwrap();
        database
            .upsert_track(
                &test_track(
                    "proton",
                    None,
                    Vec::new(),
                    root.join("steamapps/compatdata/1/pfx/junk.wav")
                        .to_str()
                        .unwrap(),
                ),
                1,
            )
            .unwrap();
        assert_eq!(database.list_tracks().unwrap().len(), 3);

        assert_eq!(
            prune_unwatched_tracks(&database, std::slice::from_ref(&root)).unwrap(),
            2
        );
        library.sync_from_database(&database).unwrap();
        assert_eq!(library.tracks().len(), 1);
        assert_eq!(library.tracks()[0].path, keep);

        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    fn test_track(
        title: &str,
        album: Option<&str>,
        artists: Vec<&str>,
        path: &str,
    ) -> TrackMetadata {
        TrackMetadata::from_display(
            path,
            title,
            album.map(ToOwned::to_owned),
            artists.into_iter().map(ToOwned::to_owned).collect(),
            Some(Duration::from_secs(10)),
        )
    }

    fn temp_music_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "qingyin-library-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
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
        fs::write(path, bytes).unwrap();
    }

    fn bump_mtime(path: &Path) {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(2);
        file.set_modified(later).unwrap();
    }
}
