mod covers;
mod watch;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use qingyin_chinese::{ReadingContext, compare_keys, sort_key, sort_key_in};
use qingyin_core::SortColumn;
use qingyin_metadata::{
    CoverArt, FileFingerprint, TrackMetadata, identity_key, preferred_display_name,
    read_tagged_track, unique_credited_artists,
};
use qingyin_storage::{Database, StorageError, TrackId};
use thiserror::Error;

pub use covers::{
    CoverFailure, CoverLookup, CoverPriority, CoverRequest, CoverScheduler, CoverService,
    CoverUpdate, commit_parsed_cover,
};
pub use watch::{LibraryWatcher, WatchRootStatus, WatchSnapshot, WatchSummary, watch_directories};

pub const UNKNOWN_ARTIST: &str = "未知歌手";
pub const UNKNOWN_ALBUM: &str = "未知专辑";
const UPSERT_BATCH_SIZE: usize = 50;

/// Precomputed collation keys for one track. Built when loading a UI snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackCollationKeys {
    pub title: String,
    pub album: String,
}

impl TrackCollationKeys {
    #[must_use]
    pub fn from_track(track: &TrackMetadata) -> Self {
        Self {
            title: sort_key(&track.title, track.title_sort.as_deref()),
            album: sort_key(
                track.album.as_deref().unwrap_or(""),
                track.album_sort.as_deref(),
            ),
        }
    }
}

/// Shared track record used by library, artist, and album views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSnapshot {
    pub metadata: Arc<TrackMetadata>,
    pub keys: TrackCollationKeys,
    pub cover_url: String,
}

impl TrackSnapshot {
    #[must_use]
    pub fn from_metadata(metadata: TrackMetadata) -> Self {
        Self::from_shared(Arc::new(metadata), String::new())
    }

    #[must_use]
    pub fn from_shared(metadata: Arc<TrackMetadata>, cover_url: String) -> Self {
        let keys = TrackCollationKeys::from_track(&metadata);
        Self {
            metadata,
            keys,
            cover_url,
        }
    }

    #[must_use]
    pub fn id(&self) -> TrackId {
        self.metadata.id
    }

    #[must_use]
    pub fn cmp_column(&self, other: &Self, column: SortColumn) -> Ordering {
        let ordering = match column {
            SortColumn::Album => compare_keys(&self.keys.album, &other.keys.album),
            SortColumn::Duration => self.metadata.duration.cmp(&other.metadata.duration),
            SortColumn::Title => compare_keys(&self.keys.title, &other.keys.title),
        };
        ordering.then_with(|| self.metadata.path.cmp(&other.metadata.path))
    }
}

impl From<TrackMetadata> for TrackSnapshot {
    fn from(metadata: TrackMetadata) -> Self {
        Self::from_metadata(metadata)
    }
}

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
    Upserted { count: usize, ids: Vec<TrackId> },
    Removed { count: usize, ids: Vec<TrackId> },
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
    pub failed_subtrees: Vec<ScanFailure>,
    pub complete: bool,
}

/// One file observed while scanning a music directory.
#[derive(Debug)]
pub enum ScanEvent<'a> {
    Imported {
        track: &'a TrackMetadata,
        cover: Option<CoverArt>,
    },
    Unchanged {
        path: &'a Path,
        fingerprint: FileFingerprint,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionEntry {
    pub id: String,
    pub name: String,
    pub subtitle: String,
    pub cover_url: String,
    pub track_count: usize,
    pub tracks: Vec<TrackSnapshot>,
}

#[derive(Debug, Default)]
pub struct MusicLibrary {
    tracks: Vec<TrackMetadata>,
}

/// Groups tracks under each listed artist. Tracks without artists use [`UNKNOWN_ARTIST`].
#[must_use]
pub fn aggregate_artists(tracks: &[TrackSnapshot]) -> Vec<CollectionEntry> {
    let mut groups = HashMap::<AlbumKey, Vec<TrackSnapshot>>::new();
    for snapshot in tracks {
        let names = unique_credited_artists(snapshot.metadata.artists.iter());
        if names.is_empty() {
            push_grouped_track(
                &mut groups,
                AlbumKey::Artist(identity_key(UNKNOWN_ARTIST)),
                snapshot.clone(),
            );
            continue;
        }
        for name in names {
            push_grouped_track(
                &mut groups,
                AlbumKey::Artist(identity_key(&name)),
                snapshot.clone(),
            );
        }
    }
    finish_collections(groups, CollectionKind::Artist)
}

/// Groups tracks by album title and album artist. Tracks without an album use an explicit unknown key.
#[must_use]
pub fn aggregate_albums(tracks: &[TrackSnapshot]) -> Vec<CollectionEntry> {
    let mut groups = HashMap::<AlbumKey, Vec<TrackSnapshot>>::new();
    for snapshot in tracks {
        let key = album_key(&snapshot.metadata);
        push_grouped_track(&mut groups, key, snapshot.clone());
    }
    finish_collections(groups, CollectionKind::Album)
}

/// Groups tracks by their immediate containing directory, preserving filesystem identity.
#[must_use]
pub fn aggregate_directories(tracks: &[TrackSnapshot]) -> Vec<CollectionEntry> {
    let mut groups = HashMap::<PathBuf, Vec<TrackSnapshot>>::new();
    for track in tracks {
        let Some(directory) = track.metadata.path.parent() else {
            continue;
        };
        groups
            .entry(directory.to_path_buf())
            .or_default()
            .push(track.clone());
    }
    let mut entries = groups
        .into_iter()
        .map(|(directory, mut tracks)| {
            tracks.sort_by(|left, right| {
                compare_keys(&left.keys.title, &right.keys.title)
                    .then_with(|| left.metadata.path.cmp(&right.metadata.path))
            });
            let path = directory.to_string_lossy();
            let name = directory.file_name().map_or_else(
                || path.to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            CollectionEntry {
                id: format!("directory:{path}"),
                name,
                subtitle: format!("{} 首歌曲 · {path}", tracks.len()),
                cover_url: tracks
                    .iter()
                    .find(|track| !track.cover_url.is_empty())
                    .map(|track| track.cover_url.clone())
                    .unwrap_or_default(),
                track_count: tracks.len(),
                tracks,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by_cached_key(|entry| (sort_key(&entry.name, None), entry.id.clone()));
    entries
}

impl MusicLibrary {
    #[must_use]
    pub fn tracks(&self) -> &[TrackMetadata] {
        &self.tracks
    }

    pub fn replace_tracks(&mut self, tracks: Vec<TrackMetadata>) {
        self.tracks = tracks;
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

/// Recursively scans a directory and persists changed audio files.
///
/// # Errors
///
/// Returns [`LibraryError`] when the directory cannot be read.
pub fn scan_directory(
    database: &mut Database,
    directory: impl AsRef<Path>,
) -> Result<ScanSummary, LibraryError> {
    scan_directory_with(database, directory, |_| {})
}

/// Scans a directory and reports each imported or unchanged file to `on_event`.
///
/// # Errors
///
/// Returns [`LibraryError`] when the directory cannot be read.
pub fn scan_directory_with(
    database: &mut Database,
    directory: impl AsRef<Path>,
    on_event: impl FnMut(ScanEvent<'_>),
) -> Result<ScanSummary, LibraryError> {
    scan_music_directory(database, directory, on_event)
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
    refresh_watched_path_with(database, path, watched_roots, |_, _| {})
}

/// Like [`refresh_watched_path`], forwarding newly parsed covers to `on_imported`.
///
/// # Errors
///
/// Returns [`LibraryError`] when the path cannot be read or storage fails.
pub fn refresh_watched_path_with(
    database: &mut Database,
    path: impl AsRef<Path>,
    watched_roots: &[PathBuf],
    on_imported: impl FnMut(&TrackMetadata, Option<&CoverArt>),
) -> Result<LibraryChange, LibraryError> {
    let path = path.as_ref();
    if !is_library_track(path, watched_roots) || is_temporary(path) {
        return Ok(LibraryChange::Ignored);
    }

    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => refresh_directory(database, path),
        Ok(_) if is_supported_audio(path) => refresh_audio_file(database, path, on_imported),
        Ok(_) => {
            if database.remove_track(path)? {
                Ok(LibraryChange::Removed {
                    count: 1,
                    ids: Vec::new(),
                })
            } else {
                Ok(LibraryChange::Ignored)
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => refresh_missing_path(database, path),
        Err(error) if is_unavailable(&error) => Ok(LibraryChange::Failed {
            path: path.to_path_buf(),
            message: error.to_string(),
        }),
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
    let directory = directory.as_ref();
    let _span = tracing::info_span!("library.scan", path = %directory.display()).entered();
    let mut paths = Vec::new();
    let mut failed_subtrees = Vec::new();
    let complete = collect_audio_paths(directory, &mut paths, &mut failed_subtrees)?;
    paths.sort_unstable();
    let existing: HashMap<PathBuf, FileFingerprint> = database
        .list_refs_under(directory)?
        .into_iter()
        .map(|track| (track.path, track.fingerprint))
        .collect();
    let mut summary = import_audio_files(database, &paths, &existing, &mut on_event);
    summary.failed_subtrees = failed_subtrees;
    summary.complete = complete && summary.failed_subtrees.is_empty();
    if summary.complete {
        reconcile_root(database, directory, &paths)?;
    }
    Ok(summary)
}

fn import_audio_files(
    database: &mut Database,
    paths: &[PathBuf],
    existing: &HashMap<PathBuf, FileFingerprint>,
    on_event: &mut impl FnMut(ScanEvent<'_>),
) -> ScanSummary {
    let mut summary = ScanSummary {
        discovered: paths.len(),
        ..ScanSummary::default()
    };
    let mut pending = Vec::new();
    for path in paths {
        let fingerprint = match file_fingerprint(path) {
            Ok(fingerprint) => fingerprint,
            Err(error) => {
                summary
                    .failed
                    .push(scan_failure(path.clone(), error.to_string()));
                continue;
            }
        };
        if existing.get(path) == Some(&fingerprint) {
            on_event(ScanEvent::Unchanged { path, fingerprint });
            summary.unchanged += 1;
            continue;
        }
        match read_tagged_track(path) {
            Ok((mut track, cover)) => {
                track.modified_at_ns = fingerprint.modified_at_ns;
                track.file_size = fingerprint.file_size;
                pending.push((track, cover));
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
    summary
}

fn flush_pending(
    database: &mut Database,
    pending: &mut Vec<(TrackMetadata, Option<CoverArt>)>,
    on_event: &mut impl FnMut(ScanEvent<'_>),
    summary: &mut ScanSummary,
) {
    if pending.is_empty() {
        return;
    }
    let batch = pending.iter().map(|(track, _)| track).collect::<Vec<_>>();
    match database.upsert_tracks(&batch) {
        Ok(()) => {
            for (track, cover) in pending.drain(..) {
                on_event(ScanEvent::Imported {
                    track: &track,
                    cover,
                });
                summary.imported += 1;
            }
            return;
        }
        Err(error) if error.is_batch_fatal() => {
            for (track, _) in pending.drain(..) {
                summary
                    .failed
                    .push(scan_failure(track.path, error.to_string()));
            }
            return;
        }
        Err(_) => {}
    }
    for (track, cover) in pending.drain(..) {
        match database.upsert_track(&track) {
            Ok(_) => {
                on_event(ScanEvent::Imported {
                    track: &track,
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
        Ok(summary) if summary.imported > 0 => Ok(LibraryChange::Upserted {
            count: summary.imported,
            ids: Vec::new(),
        }),
        Ok(summary) if !summary.failed.is_empty() && summary.unchanged == 0 => {
            Ok(LibraryChange::Failed {
                path: directory.to_path_buf(),
                message: summary.failed[0].message.clone(),
            })
        }
        Ok(_) => Ok(LibraryChange::Ignored),
        Err(LibraryError::Io { source, .. }) if source.kind() == ErrorKind::NotFound => {
            remove_missing_prefix(database, directory)
        }
        Err(error) => Err(error),
    }
}

fn refresh_audio_file(
    database: &mut Database,
    path: &Path,
    mut on_imported: impl FnMut(&TrackMetadata, Option<&CoverArt>),
) -> Result<LibraryChange, LibraryError> {
    let fingerprint = file_fingerprint(path)?;
    if database.track_fingerprint(path)? == Some(fingerprint) {
        return Ok(LibraryChange::Ignored);
    }
    match read_tagged_track(path) {
        Ok((mut track, cover)) => {
            track.modified_at_ns = fingerprint.modified_at_ns;
            track.file_size = fingerprint.file_size;
            if let Some(cover) = &cover {
                track.cover_digest = Some(cover.digest());
            }
            let id = database.upsert_track(&track)?;
            track.id = id;
            on_imported(&track, cover.as_ref());
            Ok(LibraryChange::Upserted {
                count: 1,
                ids: vec![id],
            })
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
        if let Some(existing) = database.track_fingerprint(path)? {
            let _ = existing;
        }
        let removed = database.remove_track(path)?;
        if removed {
            return Ok(LibraryChange::Removed {
                count: 1,
                ids: Vec::new(),
            });
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
    Ok(LibraryChange::Removed {
        count: removed,
        ids: Vec::new(),
    })
}

fn collect_audio_paths(
    directory: &Path,
    paths: &mut Vec<PathBuf>,
    failed_subtrees: &mut Vec<ScanFailure>,
) -> Result<bool, LibraryError> {
    collect_audio_paths_inner(directory, paths, failed_subtrees, true)
}

fn collect_audio_paths_inner(
    directory: &Path,
    paths: &mut Vec<PathBuf>,
    failed_subtrees: &mut Vec<ScanFailure>,
    is_root: bool,
) -> Result<bool, LibraryError> {
    if !is_root && is_skipped_scan_directory(directory) {
        return Ok(true);
    }

    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(source) if is_root => {
            return Err(LibraryError::Io {
                path: directory.to_path_buf(),
                source,
            });
        }
        Err(source) => {
            failed_subtrees.push(scan_failure(directory.to_path_buf(), source.to_string()));
            return Ok(false);
        }
    };

    let mut complete = true;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) => {
                failed_subtrees.push(scan_failure(directory.to_path_buf(), source.to_string()));
                complete = false;
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(source) => {
                failed_subtrees.push(scan_failure(path, source.to_string()));
                complete = false;
                continue;
            }
        };
        if file_type.is_dir() {
            complete &= collect_audio_paths_inner(&path, paths, failed_subtrees, false)?;
        } else if file_type.is_file() && is_supported_audio(&path) {
            paths.push(path);
        }
    }
    Ok(complete)
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
    database: &mut Database,
    roots: &[PathBuf],
) -> Result<usize, LibraryError> {
    if roots.is_empty() {
        return Ok(0);
    }
    let mut stale = Vec::new();
    for track in database.list_track_refs()? {
        if is_library_track(&track.path, roots) {
            continue;
        }
        stale.push(track.id);
    }
    Ok(database.remove_track_ids(&stale)?)
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

fn file_fingerprint(path: &Path) -> Result<FileFingerprint, LibraryError> {
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
        .map_or(0, |duration| {
            i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
        });
    Ok(FileFingerprint {
        modified_at_ns: timestamp,
        file_size: metadata.len(),
    })
}

fn is_unavailable(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::PermissionDenied
            | ErrorKind::TimedOut
            | ErrorKind::Interrupted
            | ErrorKind::WouldBlock
            | ErrorKind::UnexpectedEof
    )
}

fn reconcile_root(
    database: &mut Database,
    directory: &Path,
    discovered: &[PathBuf],
) -> Result<usize, LibraryError> {
    let discovered: HashSet<&Path> = discovered.iter().map(PathBuf::as_path).collect();
    let mut stale = Vec::new();
    for track in database.list_refs_under(directory)? {
        if !discovered.contains(track.path.as_path()) {
            stale.push(track.id);
        }
    }
    Ok(database.remove_track_ids(&stale)?)
}

/// Deduplicates and drops nested roots while keeping offline paths that cannot be canonicalized.
#[must_use]
pub fn normalize_roots(directories: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for directory in directories {
        if directory.as_os_str().is_empty() {
            continue;
        }
        let canonical = directory
            .canonicalize()
            .unwrap_or_else(|_| directory.clone());
        if roots.iter().any(|existing: &PathBuf| {
            paths_equal(existing, &canonical) || is_nested_under(&canonical, existing)
        }) {
            continue;
        }
        roots.retain(|existing| !is_nested_under(existing, &canonical));
        roots.push(canonical);
    }
    roots
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
}

fn is_nested_under(path: &Path, root: &Path) -> bool {
    path != root && path.starts_with(root)
}

fn scan_failure(path: PathBuf, message: String) -> ScanFailure {
    ScanFailure { path, message }
}

#[derive(Clone, Copy)]
enum CollectionKind {
    Artist,
    Album,
}

fn push_grouped_track(
    groups: &mut HashMap<AlbumKey, Vec<TrackSnapshot>>,
    key: AlbumKey,
    snapshot: TrackSnapshot,
) {
    groups.entry(key).or_default().push(snapshot);
}

fn finish_collections(
    groups: HashMap<AlbumKey, Vec<TrackSnapshot>>,
    kind: CollectionKind,
) -> Vec<CollectionEntry> {
    let mut groups = groups
        .into_iter()
        .map(|(key, tracks)| {
            let name = collection_display_name(&key, &tracks);
            let sort = collection_name_key(&name, &tracks, kind);
            (key, name, sort, tracks)
        })
        .collect::<Vec<_>>();
    groups.sort_by(
        |(left_key, left_name, left_sort, _), (right_key, right_name, right_sort, _)| {
            compare_collection_name(left_name, left_sort, right_name, right_sort)
                .then_with(|| left_key.stable_id().cmp(&right_key.stable_id()))
        },
    );
    groups
        .into_iter()
        .map(|(key, name, _, mut tracks)| {
            tracks.sort_by(|left, right| compare_grouped_tracks(left, right, kind));
            let cover_url = tracks
                .iter()
                .find_map(|item| (!item.cover_url.is_empty()).then(|| item.cover_url.clone()))
                .unwrap_or_default();
            let subtitle = collection_subtitle(&name, &tracks, kind);
            let track_count = tracks.len();
            CollectionEntry {
                id: key.stable_id(),
                name,
                subtitle,
                cover_url,
                track_count,
                tracks,
            }
        })
        .collect()
}

fn collection_subtitle(name: &str, tracks: &[TrackSnapshot], kind: CollectionKind) -> String {
    match kind {
        CollectionKind::Artist => format!("{} 首歌曲", tracks.len()),
        CollectionKind::Album => {
            let artists = unique_artists(tracks);
            if artists.is_empty() {
                format!("{name} · {} 首歌曲", tracks.len())
            } else {
                format!("{} · {} 首歌曲", artists.join("、"), tracks.len())
            }
        }
    }
}

fn unique_artists(tracks: &[TrackSnapshot]) -> Vec<String> {
    let mut artists =
        unique_credited_artists(tracks.iter().flat_map(|item| item.metadata.artists.iter()))
            .into_iter()
            .map(|name| {
                let key = sort_key_in(&name, None, ReadingContext::PersonName);
                (name, key)
            })
            .collect::<Vec<_>>();
    artists.sort_by(|left, right| compare_keys(&left.1, &right.1));
    artists.into_iter().map(|(name, _)| name).collect()
}

fn compare_collection_name(left: &str, left_key: &str, right: &str, right_key: &str) -> Ordering {
    match (is_unknown_collection(left), is_unknown_collection(right)) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => compare_keys(left_key, right_key),
    }
}

fn collection_name_key(name: &str, tracks: &[TrackSnapshot], kind: CollectionKind) -> String {
    sort_key(name, collection_sort_tag(name, tracks, kind))
}

fn collection_sort_tag<'a>(
    name: &str,
    tracks: &'a [TrackSnapshot],
    kind: CollectionKind,
) -> Option<&'a str> {
    let identity = identity_key(name);
    tracks.iter().find_map(|item| match kind {
        CollectionKind::Artist
            if item.metadata.artists.len() == 1
                && item
                    .metadata
                    .artists
                    .first()
                    .is_some_and(|artist| identity_key(artist) == identity) =>
        {
            nonempty_sort_tag(item.metadata.artist_sort.as_deref())
        }
        CollectionKind::Album
            if item
                .metadata
                .album
                .as_deref()
                .is_some_and(|album| identity_key(album) == identity) =>
        {
            nonempty_sort_tag(item.metadata.album_sort.as_deref())
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
    left: &TrackSnapshot,
    right: &TrackSnapshot,
    kind: CollectionKind,
) -> Ordering {
    match kind {
        CollectionKind::Artist => compare_keys(&left.keys.album, &right.keys.album)
            .then_with(|| compare_keys(&left.keys.title, &right.keys.title))
            .then_with(|| left.metadata.path.cmp(&right.metadata.path)),
        CollectionKind::Album => left
            .metadata
            .disc_number
            .cmp(&right.metadata.disc_number)
            .then_with(|| left.metadata.track_number.cmp(&right.metadata.track_number))
            .then_with(|| compare_keys(&left.keys.title, &right.keys.title))
            .then_with(|| left.metadata.id.cmp(&right.metadata.id))
            .then_with(|| left.metadata.path.cmp(&right.metadata.path)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum AlbumKey {
    Artist(String),
    Album { title: String, album_artist: String },
    UnknownAlbum,
}

fn album_key(track: &TrackMetadata) -> AlbumKey {
    let Some(title) = track
        .album
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return AlbumKey::UnknownAlbum;
    };
    AlbumKey::Album {
        title: identity_key(title),
        album_artist: identity_key(track.album_artist.as_deref().unwrap_or_default().trim()),
    }
}

fn collection_display_name(key: &AlbumKey, tracks: &[TrackSnapshot]) -> String {
    match key {
        AlbumKey::UnknownAlbum => UNKNOWN_ALBUM.to_owned(),
        AlbumKey::Artist(identity) if identity == UNKNOWN_ARTIST => UNKNOWN_ARTIST.to_owned(),
        AlbumKey::Artist(identity) => preferred_display_name(tracks.iter().flat_map(|item| {
            unique_credited_artists(item.metadata.artists.iter())
                .into_iter()
                .filter(|name| identity_key(name) == *identity)
        }))
        .unwrap_or_else(|| identity.clone()),
        AlbumKey::Album { title, .. } => preferred_display_name(
            tracks
                .iter()
                .filter_map(|item| item.metadata.album.as_deref())
                .filter(|name| identity_key(name) == *title),
        )
        .unwrap_or_else(|| title.clone()),
    }
}

impl AlbumKey {
    fn stable_id(&self) -> String {
        match self {
            Self::Artist(name) if name == UNKNOWN_ARTIST => "artist:unknown".into(),
            Self::Artist(name) => format!("artist:{name}"),
            Self::UnknownAlbum => "album:unknown".into(),
            Self::Album {
                title,
                album_artist,
            } => format!("album:{title}\u{1f}{album_artist}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn directory_groups_keep_exact_parents_and_shared_tracks() {
        let tracks = snapshots_with_covers(
            vec![
                test_track("B", None, vec![], "/music/Live/b.flac"),
                test_track("A", None, vec![], "/music/Live/a.flac"),
                test_track("C", None, vec![], "/other/Live/c.flac"),
                test_track("D", None, vec![], "/music/live/d.flac"),
                test_track("E", None, vec![], "/music/Live/Disc 2/e.flac"),
            ],
            &["cover-b", "", "", "", ""],
        );
        let directories = aggregate_directories(&tracks);
        assert_eq!(directories.len(), 4);
        let live = directories
            .iter()
            .find(|entry| entry.id == "directory:/music/Live")
            .unwrap();
        assert_eq!(live.name, "Live");
        assert_eq!(live.track_count, 2);
        assert_eq!(live.subtitle, "2 首歌曲 · /music/Live");
        assert_eq!(live.cover_url, "cover-b");
        assert_eq!(live.tracks[0].metadata.title, "A");
        assert!(Arc::ptr_eq(&live.tracks[0].metadata, &tracks[1].metadata));
        let mut reversed = tracks.clone();
        reversed.reverse();
        assert_eq!(directories, aggregate_directories(&reversed));
        assert!(aggregate_directories(&[]).is_empty());
    }

    #[test]
    fn recognizes_supported_extensions_case_insensitively() {
        assert!(is_supported_audio(Path::new("track.FLAC")));
        assert!(is_supported_audio(Path::new("track.mp3")));
        assert!(!is_supported_audio(Path::new("cover.jpg")));
        assert!(!is_supported_audio(Path::new("lyrics.lrc")));
    }

    #[test]
    fn aggregates_artists_and_albums_from_library_tracks() {
        let tracks = snapshots_with_covers(
            vec![
                test_track(
                    "夜色",
                    Some("清音"),
                    vec!["乙歌手", "甲歌手"],
                    "/music/night.flac",
                ),
                test_track("晨光", Some("清音"), vec!["甲歌手"], "/music/morning.flac"),
                test_track("无题", None, Vec::new(), "/music/untitled.flac"),
            ],
            &["night-cover", "morning-cover", ""],
        );

        let artists = aggregate_artists(&tracks);
        assert_eq!(artists.len(), 3);
        assert_eq!(artists[0].name, "甲歌手");
        assert_eq!(artists[0].track_count, 2);
        assert_eq!(artists[0].cover_url, "morning-cover");
        assert_eq!(artists[0].tracks[0].metadata.title, "晨光");
        assert_eq!(artists[0].tracks[1].metadata.title, "夜色");
        assert_eq!(artists[1].name, "乙歌手");
        assert_eq!(artists[2].name, UNKNOWN_ARTIST);
        assert_eq!(artists[2].track_count, 1);

        let albums = aggregate_albums(&tracks);
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].name, "清音");
        assert_eq!(albums[0].track_count, 2);
        assert_eq!(albums[0].subtitle, "甲歌手、乙歌手 · 2 首歌曲");
        assert_eq!(albums[0].cover_url, "morning-cover");
        assert_eq!(albums[0].tracks[0].metadata.title, "晨光");
        assert_eq!(albums[0].tracks[1].metadata.title, "夜色");
        assert_eq!(albums[1].name, UNKNOWN_ALBUM);
        assert_eq!(albums[1].track_count, 1);
    }

    #[test]
    fn same_album_title_splits_when_album_artists_differ() {
        let mut jay = test_track("晴天", Some("叶惠美"), vec!["周杰伦"], "/music/jay.flac");
        jay.album_artist = Some("周杰伦".into());
        let mut other = test_track("晴天", Some("叶惠美"), vec!["翻唱"], "/music/cover.flac");
        other.album_artist = Some("翻唱合集".into());
        let albums = aggregate_albums(&snapshots(vec![jay, other]));
        assert_eq!(albums.len(), 2);
        assert!(albums.iter().all(|album| album.name == "叶惠美"));
        assert_ne!(albums[0].id, albums[1].id);
    }

    #[test]
    fn artist_sort_tags_reorder_collection_names() {
        let mut jay = test_track("晴天", Some("叶惠美"), vec!["周杰伦"], "/music/jay.flac");
        jay.artist_sort = Some("Aaa".into());
        let li = test_track("麻雀", Some("麻雀"), vec!["李荣浩"], "/music/li.flac");
        let artists = aggregate_artists(&snapshots(vec![jay, li]));
        assert_eq!(artists[0].name, "周杰伦");
        assert_eq!(artists[1].name, "李荣浩");
    }

    #[test]
    fn artist_collections_interleave_han_and_latin_names() {
        let tracks = snapshots(vec![
            test_track("晴天", None, vec!["周杰伦"], "/music/jay.flac"),
            test_track("Hello", None, vec!["Adele"], "/music/adele.flac"),
            test_track("听海", None, vec!["阿妹"], "/music/amei.flac"),
        ]);
        let artists = aggregate_artists(&tracks);
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
        let tracks = snapshots(vec![test_track(
            "夜色",
            Some("清音"),
            vec!["乙歌手", "甲歌手"],
            "/music/night.flac",
        )]);
        let artists = aggregate_artists(&tracks);
        assert_eq!(artists.len(), 2);
        assert!(Arc::ptr_eq(
            &artists[0].tracks[0].metadata,
            &artists[1].tracks[0].metadata
        ));
    }

    #[test]
    fn splits_joined_artist_credits_when_aggregating() {
        let tracks = snapshots(vec![test_track(
            "夜色",
            Some("清音"),
            vec!["Singer A、Singer B"],
            "/music/night.flac",
        )]);
        let artists = aggregate_artists(&tracks);
        assert_eq!(
            artists
                .iter()
                .map(|artist| artist.name.as_str())
                .collect::<Vec<_>>(),
            ["Singer A", "Singer B"]
        );
        assert!(artists.iter().all(|artist| artist.track_count == 1));
        let albums = aggregate_albums(&tracks);
        assert_eq!(albums[0].subtitle, "Singer A、Singer B · 1 首歌曲");
    }

    #[test]
    fn merges_case_variant_artists_and_albums_using_most_uppercase_name() {
        let tracks = snapshots(vec![
            test_track("One", Some("revival"), vec!["or3o"], "/music/one.flac"),
            test_track("Two", Some("Revival"), vec!["OR3O"], "/music/two.flac"),
            test_track("Three", Some("REVIVAL"), vec!["Or3o"], "/music/three.flac"),
        ]);
        let artists = aggregate_artists(&tracks);
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].name, "OR3O");
        assert_eq!(artists[0].track_count, 3);
        assert_eq!(artists[0].id, "artist:or3o");

        let albums = aggregate_albums(&tracks);
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].name, "REVIVAL");
        assert_eq!(albums[0].track_count, 3);
        assert_eq!(albums[0].subtitle, "OR3O · 3 首歌曲");
    }

    #[test]
    fn refresh_path_ignores_temporary_unwatched_and_non_audio_files() {
        let mut database = Database::open_in_memory().unwrap();
        let root = temp_music_dir();
        let roots = [root.clone()];
        let outside = std::env::temp_dir().join("qingyin-outside.flac");
        let image = root.join("cover.jpg");
        let partial = root.join("track.flac.part");
        fs::write(&image, b"not-audio").unwrap();
        fs::write(&partial, b"partial").unwrap();

        assert_eq!(
            refresh_watched_path(&mut database, &outside, &roots).unwrap(),
            LibraryChange::Ignored
        );
        assert_eq!(
            refresh_watched_path(&mut database, &partial, &roots).unwrap(),
            LibraryChange::Ignored
        );
        assert_eq!(
            refresh_watched_path(&mut database, &image, &roots).unwrap(),
            LibraryChange::Ignored
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refresh_path_imports_updates_and_removes_audio_files() {
        let mut database = Database::open_in_memory().unwrap();
        let root = temp_music_dir();
        let roots = [root.clone()];
        let path = root.join("清音.wav");
        write_silence_wav(&path);

        assert!(matches!(
            refresh_watched_path(&mut database, &path, &roots).unwrap(),
            LibraryChange::Upserted { count: 1, .. }
        ));
        assert_eq!(database.list_tracks().unwrap().len(), 1);
        assert_eq!(
            refresh_watched_path(&mut database, &path, &roots).unwrap(),
            LibraryChange::Ignored
        );

        bump_mtime(&path);
        assert!(matches!(
            refresh_watched_path(&mut database, &path, &roots).unwrap(),
            LibraryChange::Upserted { count: 1, .. }
        ));

        fs::remove_file(&path).unwrap();
        assert!(matches!(
            refresh_watched_path(&mut database, &path, &roots).unwrap(),
            LibraryChange::Removed { count: 1, .. }
        ));
        assert!(database.list_tracks().unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refresh_path_removes_tracks_when_a_directory_disappears() {
        let mut database = Database::open_in_memory().unwrap();
        let root = temp_music_dir();
        let album = root.join("album");
        fs::create_dir_all(&album).unwrap();
        let nested = album.join("track.wav");
        write_silence_wav(&nested);
        let roots = [root.clone()];

        assert!(matches!(
            refresh_watched_path(&mut database, &album, &roots).unwrap(),
            LibraryChange::Upserted { count: 1, .. }
        ));

        fs::remove_dir_all(&album).unwrap();
        assert!(matches!(
            refresh_watched_path(&mut database, &album, &roots).unwrap(),
            LibraryChange::Removed { count: 1, .. }
        ));
        assert!(database.list_tracks().unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_skips_proton_and_virtualenv_directories() {
        let mut database = Database::open_in_memory().unwrap();
        let root = temp_music_dir();
        write_silence_wav(&root.join("keep.wav"));

        let proton = root.join("steamapps/compatdata/1/pfx");
        fs::create_dir_all(&proton).unwrap();
        write_silence_wav(&proton.join("junk.wav"));

        let venv = root.join(".venv/lib/site-packages");
        fs::create_dir_all(&venv).unwrap();
        write_silence_wav(&venv.join("also.wav"));

        scan_directory(&mut database, &root).unwrap();
        let tracks = database.list_tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].metadata.path.file_name().unwrap(), "keep.wav");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refresh_path_ignores_audio_under_skipped_directories() {
        let mut database = Database::open_in_memory().unwrap();
        let root = temp_music_dir();
        let proton = root.join("steamapps/compatdata/1/pfx");
        fs::create_dir_all(&proton).unwrap();
        let junk = proton.join("junk.wav");
        write_silence_wav(&junk);
        let roots = [root.clone()];

        assert_eq!(
            refresh_watched_path(&mut database, &junk, &roots).unwrap(),
            LibraryChange::Ignored
        );
        assert!(database.list_tracks().unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_removes_tracks_outside_roots_and_skipped_trees() {
        let mut database = Database::open_in_memory().unwrap();
        let root = temp_music_dir();
        let keep = root.join("keep.wav");
        write_silence_wav(&keep);
        scan_directory(&mut database, &root).unwrap();

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
            .upsert_track(&test_track(
                "outside",
                None,
                Vec::new(),
                outside.to_str().unwrap(),
            ))
            .unwrap();
        database
            .upsert_track(&test_track(
                "proton",
                None,
                Vec::new(),
                root.join("steamapps/compatdata/1/pfx/junk.wav")
                    .to_str()
                    .unwrap(),
            ))
            .unwrap();
        assert_eq!(database.list_tracks().unwrap().len(), 3);

        assert_eq!(
            prune_unwatched_tracks(&mut database, std::slice::from_ref(&root)).unwrap(),
            2
        );
        let tracks = database.list_tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].metadata.path, keep);

        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn snapshot_keys_use_tags_and_pinyin() {
        let mut track = test_track("周杰伦", Some("叶惠美"), vec!["周杰伦"], "/music/a.flac");
        track.title_sort = Some("Jay Chou".into());
        track.artist_sort = Some("Jay Chou".into());
        let snapshot = TrackSnapshot::from_metadata(track);
        assert_eq!(snapshot.keys.title, "Jay Chou");
        assert_eq!(snapshot.keys.album.to_lowercase(), "yehuimei");
    }

    fn snapshots(tracks: Vec<TrackMetadata>) -> Vec<TrackSnapshot> {
        tracks
            .into_iter()
            .map(TrackSnapshot::from_metadata)
            .collect()
    }

    fn snapshots_with_covers(tracks: Vec<TrackMetadata>, covers: &[&str]) -> Vec<TrackSnapshot> {
        tracks
            .into_iter()
            .zip(covers)
            .map(|(track, cover)| TrackSnapshot::from_shared(Arc::new(track), (*cover).to_owned()))
            .collect()
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
        let file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        let later = std::time::SystemTime::now() + Duration::from_millis(5);
        file.set_modified(later).unwrap();
        use std::io::Write;
        let mut file = file;
        file.write_all(b"\0").unwrap();
    }

    #[test]
    fn permission_denied_subtree_does_not_delete_records() {
        let mut database = Database::open_in_memory().unwrap();
        let root = temp_music_dir();
        write_silence_wav(&root.join("keep.wav"));
        scan_directory(&mut database, &root).unwrap();
        assert_eq!(database.list_tracks().unwrap().len(), 1);

        let hidden = root.join("hidden");
        fs::create_dir_all(&hidden).unwrap();
        write_silence_wav(&hidden.join("secret.wav"));
        let mut perms = fs::metadata(&hidden).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o000);
            fs::set_permissions(&hidden, perms).unwrap();
        }
        let change =
            refresh_watched_path(&mut database, &root, std::slice::from_ref(&root)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut restore = fs::metadata(&hidden).unwrap().permissions();
            restore.set_mode(0o755);
            let _ = fs::set_permissions(&hidden, restore);
        }
        assert_ne!(
            change,
            LibraryChange::Removed {
                count: 1,
                ids: Vec::new()
            }
        );
        assert_eq!(database.list_tracks().unwrap().len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn complete_scan_reconciles_offline_deletes() {
        let mut database = Database::open_in_memory().unwrap();
        let root = temp_music_dir();
        let keep = root.join("keep.wav");
        let gone = root.join("gone.wav");
        write_silence_wav(&keep);
        write_silence_wav(&gone);
        scan_directory(&mut database, &root).unwrap();
        fs::remove_file(&gone).unwrap();
        let summary = scan_directory(&mut database, &root).unwrap();
        assert!(summary.complete);
        let tracks = database.list_tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].metadata.path, keep);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn album_details_sort_by_disc_then_track() {
        let mut late = test_track("后", Some("碟"), vec!["甲"], "/music/late.flac");
        late.disc_number = Some(2);
        late.track_number = Some(1);
        let mut early = test_track("前", Some("碟"), vec!["甲"], "/music/early.flac");
        early.disc_number = Some(1);
        early.track_number = Some(8);
        let albums = aggregate_albums(&snapshots(vec![late, early]));
        assert_eq!(albums[0].tracks[0].metadata.title, "前");
        assert_eq!(albums[0].tracks[1].metadata.title, "后");
    }

    #[test]
    fn nested_roots_collapse_without_merging_case_variants() {
        let roots = normalize_roots(vec![
            PathBuf::from("/music"),
            PathBuf::from("/music/album"),
            PathBuf::from("/Music"),
        ]);
        assert!(roots.iter().any(|path| path == Path::new("/music")));
        assert!(roots.iter().any(|path| path == Path::new("/Music")));
        assert!(!roots.iter().any(|path| path == Path::new("/music/album")));
    }

    #[test]
    fn empty_roots_do_not_prune_existing_tracks() {
        let mut database = Database::open_in_memory().unwrap();
        database
            .upsert_track(&test_track("a", None, Vec::new(), "/music/a.flac"))
            .unwrap();
        assert_eq!(prune_unwatched_tracks(&mut database, &[]).unwrap(), 0);
        assert_eq!(database.list_tracks().unwrap().len(), 1);
    }
}
