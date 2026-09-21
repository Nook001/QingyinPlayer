use std::collections::BTreeSet;
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::event::ModifyKind;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::warn;

use crate::covers::CoverService;
use crate::{
    CollectionEntry, LibraryChange, LibraryError, TrackSnapshot, aggregate_albums,
    aggregate_artists, aggregate_directories, commit_parsed_cover, normalize_roots,
    refresh_watched_path_with,
};
use qingyin_storage::Database;

const QUIET_WINDOW: Duration = Duration::from_millis(400);
const MAX_WAIT: Duration = Duration::from_secs(2);
const EVENT_MAILBOX: usize = 256;
const SHUTDOWN_JOIN: Duration = Duration::from_secs(2);
const INITIAL_RETRY: Duration = Duration::from_secs(2);
const MAX_RETRY: Duration = Duration::from_secs(30);

/// Incremental library changes collected from a debounced watcher batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchSnapshot {
    pub summary: WatchSummary,
    pub tracks: Vec<TrackSnapshot>,
    pub artists: Vec<CollectionEntry>,
    pub albums: Vec<CollectionEntry>,
    pub directories: Vec<CollectionEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchSummary {
    pub upserted: usize,
    pub removed: usize,
    pub failed: usize,
    pub overflow: bool,
    pub needs_reconcile: bool,
    pub upserted_ids: Vec<i64>,
    pub removed_ids: Vec<i64>,
    pub roots: Vec<WatchRootStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRootStatus {
    pub path: PathBuf,
    pub watching: bool,
    pub error: Option<String>,
}

impl WatchSummary {
    #[must_use]
    pub const fn has_library_changes(&self) -> bool {
        self.upserted > 0 || self.removed > 0 || self.needs_reconcile
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.upserted == 0 && self.removed == 0 && self.failed == 0 && !self.overflow
    }

    #[must_use]
    pub fn all_roots_failed(&self) -> bool {
        !self.roots.is_empty() && self.roots.iter().all(|root| !root.watching)
    }
}

/// True when a previously failed watch root is a directory again (remounted or restored).
#[must_use]
pub(crate) fn remounted_failed_roots(roots: &[WatchRootStatus]) -> bool {
    roots
        .iter()
        .any(|root| !root.watching && root.path.is_dir())
}

#[must_use]
pub(crate) fn next_retry_wait(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_RETRY)
}

enum WatchMessage {
    Paths(Vec<PathBuf>),
    Overflow,
    Stop,
}

/// Recursively watches music directories and refreshes changed paths off the UI thread.
pub struct LibraryWatcher {
    command_tx: SyncSender<WatchMessage>,
    worker: Option<JoinHandle<()>>,
    _watcher: RecommendedWatcher,
    root_status: Vec<WatchRootStatus>,
}

impl Debug for LibraryWatcher {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LibraryWatcher")
            .finish_non_exhaustive()
    }
}

impl LibraryWatcher {
    #[must_use]
    pub fn root_status(&self) -> &[WatchRootStatus] {
        &self.root_status
    }
}

impl Drop for LibraryWatcher {
    fn drop(&mut self) {
        let _ = self.command_tx.try_send(WatchMessage::Stop);
        if let Some(worker) = self.worker.take() {
            let done = std::sync::mpsc::sync_channel(1);
            let (tx, rx) = done;
            thread::spawn(move || {
                let _ = worker.join();
                let _ = tx.send(());
            });
            let _ = rx.recv_timeout(SHUTDOWN_JOIN);
        }
    }
}

/// Starts a recursive watcher for `directories`.
///
/// Filesystem events are coalesced with a quiet window and a maximum wait, then applied
/// with [`refresh_watched_path`] on a worker thread that owns its SQLite connection.
///
/// # Errors
///
/// Returns [`LibraryError`] when the native watcher cannot be created.
pub fn watch_directories(
    directories: Vec<PathBuf>,
    database_path: PathBuf,
    on_batch: impl Fn(WatchSnapshot) + Send + 'static,
) -> Result<LibraryWatcher, LibraryError> {
    let roots = normalize_roots(directories);
    let (command_tx, command_rx) = mpsc::sync_channel(EVENT_MAILBOX);
    let event_tx = command_tx.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<Event>| match result {
            Ok(event) => {
                let paths = event_paths(&event);
                if paths.is_empty() {
                    return;
                }
                match event_tx.try_send(WatchMessage::Paths(paths)) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        let _ = event_tx.try_send(WatchMessage::Overflow);
                    }
                    Err(TrySendError::Disconnected(_)) => {}
                }
            }
            Err(error) => warn!(%error, "library watcher error"),
        },
        Config::default(),
    )?;

    let mut root_status = Vec::new();
    for directory in &roots {
        if !directory.is_dir() {
            root_status.push(WatchRootStatus {
                path: directory.clone(),
                watching: false,
                error: Some("目录当前不可用".into()),
            });
            continue;
        }
        match watcher.watch(directory, RecursiveMode::Recursive) {
            Ok(()) => root_status.push(WatchRootStatus {
                path: directory.clone(),
                watching: true,
                error: None,
            }),
            Err(error) => {
                warn!(
                    path = %directory.display(),
                    %error,
                    "failed to watch music directory"
                );
                root_status.push(WatchRootStatus {
                    path: directory.clone(),
                    watching: false,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    let worker_roots = roots;
    let worker_status = root_status.clone();
    let worker = thread::spawn(move || {
        let mut pending = BTreeSet::new();
        let mut quiet_deadline: Option<Instant> = None;
        let mut max_deadline: Option<Instant> = None;
        let mut overflow = false;
        let mut retry_wait = INITIAL_RETRY;
        let mut poll_failed = worker_status.iter().any(|root| !root.watching);
        let mut remount_signaled = false;
        loop {
            let wait = match (quiet_deadline, max_deadline) {
                (Some(quiet), Some(max)) => {
                    Some(quiet.min(max).saturating_duration_since(Instant::now()))
                }
                (Some(quiet), None) => Some(quiet.saturating_duration_since(Instant::now())),
                (None, Some(max)) => Some(max.saturating_duration_since(Instant::now())),
                (None, None) => None,
            };
            let received = match wait {
                Some(timeout) => match command_rx.recv_timeout(timeout) {
                    Ok(message) => Some(Ok(message)),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                None if poll_failed => match command_rx.recv_timeout(retry_wait) {
                    Ok(message) => Some(Ok(message)),
                    Err(RecvTimeoutError::Timeout) => Some(Err(())),
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                None => match command_rx.recv() {
                    Ok(message) => Some(Ok(message)),
                    Err(_) => break,
                },
            };

            match received {
                Some(Ok(WatchMessage::Stop)) => break,
                Some(Ok(WatchMessage::Overflow)) => {
                    overflow = true;
                    pending.clear();
                    quiet_deadline = Some(Instant::now() + QUIET_WINDOW);
                    max_deadline.get_or_insert_with(|| Instant::now() + MAX_WAIT);
                }
                Some(Ok(WatchMessage::Paths(paths))) => {
                    pending.extend(paths);
                    merge_covering_paths(&mut pending);
                    let now = Instant::now();
                    quiet_deadline = Some(now + QUIET_WINDOW);
                    max_deadline.get_or_insert(now + MAX_WAIT);
                }
                Some(Err(())) => {
                    if remounted_failed_roots(&worker_status) {
                        if !remount_signaled {
                            remount_signaled = true;
                            let summary = WatchSummary {
                                needs_reconcile: true,
                                roots: worker_status.clone(),
                                ..WatchSummary::default()
                            };
                            on_batch(build_watch_snapshot(&database_path, &worker_roots, summary));
                        }
                        poll_failed = false;
                        retry_wait = INITIAL_RETRY;
                    } else {
                        remount_signaled = false;
                        retry_wait = next_retry_wait(retry_wait);
                    }
                }
                None => {
                    let mut summary = if overflow {
                        WatchSummary {
                            overflow: true,
                            needs_reconcile: true,
                            roots: worker_status.clone(),
                            ..WatchSummary::default()
                        }
                    } else {
                        apply_pending(&database_path, &worker_roots, &mut pending)
                    };
                    summary.roots.clone_from(&worker_status);
                    if overflow {
                        overflow = false;
                    }
                    if !summary.is_empty() || summary.needs_reconcile {
                        on_batch(build_watch_snapshot(&database_path, &worker_roots, summary));
                    }
                    quiet_deadline = None;
                    max_deadline = None;
                }
            }
        }
    });

    Ok(LibraryWatcher {
        command_tx,
        worker: Some(worker),
        _watcher: watcher,
        root_status,
    })
}

fn apply_pending(
    database_path: &Path,
    roots: &[PathBuf],
    pending: &mut BTreeSet<PathBuf>,
) -> WatchSummary {
    let paths = merge_covering_paths_vec(std::mem::take(pending).into_iter().collect());
    if paths.is_empty() {
        return WatchSummary::default();
    }
    apply_paths(database_path, roots, &paths)
}

fn apply_paths(database_path: &Path, roots: &[PathBuf], paths: &[PathBuf]) -> WatchSummary {
    let mut summary = WatchSummary::default();
    let mut database = match Database::open_migrated(database_path) {
        Ok(database) => database,
        Err(error) => {
            warn!(%error, "library watcher could not open database");
            summary.failed = summary.failed.saturating_add(1);
            return summary;
        }
    };
    let covers = CoverService::open_default().ok();
    for path in paths {
        match refresh_watched_path_with(&mut database, path, roots, |track, cover| {
            if let Some(service) = &covers {
                let mut track = track.clone();
                let _ = commit_parsed_cover(service, &mut track, cover.cloned());
            }
        }) {
            Ok(LibraryChange::Upserted { count, ids }) => {
                summary.upserted += count;
                summary.upserted_ids.extend(ids);
            }
            Ok(LibraryChange::Removed { count, ids }) => {
                summary.removed += count;
                summary.removed_ids.extend(ids);
            }
            Ok(LibraryChange::Ignored) => {}
            Ok(LibraryChange::Failed { path, message }) => {
                warn!(path = %path.display(), %message, "library refresh failed");
                summary.failed += 1;
            }
            Err(error) => {
                warn!(path = %path.display(), %error, "library refresh error");
                summary.failed += 1;
            }
        }
    }
    summary
}

fn build_watch_snapshot(
    database_path: &Path,
    roots: &[PathBuf],
    summary: WatchSummary,
) -> WatchSnapshot {
    let covers = CoverService::open_default().ok();
    let mut tracks = Vec::new();
    if let Ok(database) = Database::open_migrated(database_path)
        && let Ok(stored) = database.list_tracks()
    {
        tracks = stored
            .into_iter()
            .map(|stored| {
                let mut metadata = stored.metadata;
                metadata.id = stored.id;
                let cover_url = covers
                    .as_ref()
                    .and_then(|service| match service.lookup(&metadata) {
                        crate::covers::CoverLookup::Ready(url) => Some(url),
                        _ => None,
                    })
                    .unwrap_or_default();
                TrackSnapshot::from_shared(std::sync::Arc::new(metadata), cover_url)
            })
            .collect();
    }
    let artists = aggregate_artists(&tracks);
    let albums = aggregate_albums(&tracks);
    let directories = aggregate_directories(&tracks, roots);
    WatchSnapshot {
        summary,
        tracks,
        artists,
        albums,
        directories,
    }
}

fn merge_covering_paths(pending: &mut BTreeSet<PathBuf>) {
    *pending = merge_covering_paths_vec(pending.iter().cloned().collect())
        .into_iter()
        .collect();
}

fn merge_covering_paths_vec(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    let mut merged = Vec::<PathBuf>::new();
    for path in paths {
        if merged
            .last()
            .is_some_and(|parent| path == *parent || path.starts_with(parent))
        {
            continue;
        }
        merged.push(path);
    }
    merged
}

fn event_paths(event: &Event) -> Vec<PathBuf> {
    if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
        return Vec::new();
    }
    event
        .paths
        .iter()
        .cloned()
        .map(|path| path.canonicalize().unwrap_or(path))
        .filter(|path| !should_skip_existing_directory(event, path))
        .collect()
}

fn should_skip_existing_directory(event: &Event, path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
    )
}

#[cfg(test)]
mod tests {
    use super::{
        INITIAL_RETRY, MAX_RETRY, WatchRootStatus, event_paths, merge_covering_paths_vec,
        next_retry_wait, remounted_failed_roots, watch_directories,
    };
    use crate::{is_temporary, is_watched_path, normalize_roots};
    use notify::event::{AccessKind, CreateKind, Event, EventKind, ModifyKind};
    use qingyin_storage::Database;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn access_events_do_not_produce_refresh_paths() {
        let event = Event::new(EventKind::Access(AccessKind::Any)).add_path("/music/a.flac".into());
        assert!(event_paths(&event).is_empty());
    }

    #[test]
    fn create_events_keep_their_paths() {
        let event =
            Event::new(EventKind::Create(CreateKind::File)).add_path("/music/a.flac".into());
        assert_eq!(event_paths(&event), vec![PathBuf::from("/music/a.flac")]);
    }

    #[test]
    fn directory_modify_events_are_skipped() {
        let directory = std::env::temp_dir();
        let canonical = directory
            .canonicalize()
            .unwrap_or_else(|_| directory.clone());
        let modify = Event::new(EventKind::Modify(ModifyKind::Any)).add_path(directory.clone());
        assert!(event_paths(&modify).is_empty());

        let create = Event::new(EventKind::Create(CreateKind::Folder)).add_path(directory);
        assert_eq!(event_paths(&create), vec![canonical]);
    }

    #[test]
    fn temporary_and_unwatched_paths_are_filtered() {
        assert!(is_temporary(std::path::Path::new("/music/track.flac.part")));
        assert!(is_temporary(std::path::Path::new("/music/track.tmp")));
        assert!(!is_temporary(std::path::Path::new("/music/track.flac")));

        let roots = [PathBuf::from("/music")];
        assert!(is_watched_path(std::path::Path::new("/music"), &roots));
        assert!(is_watched_path(
            std::path::Path::new("/music/album/a.flac"),
            &roots
        ));
        assert!(!is_watched_path(
            std::path::Path::new("/music-live/a.flac"),
            &roots
        ));
    }

    #[test]
    fn unique_roots_canonicalize_when_possible() {
        let roots = normalize_roots(vec![PathBuf::from("/music"), PathBuf::from("/music")]);
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn parent_paths_cover_child_events() {
        let merged = merge_covering_paths_vec(vec![
            PathBuf::from("/music/album"),
            PathBuf::from("/music/album/a.flac"),
            PathBuf::from("/music/other.flac"),
        ]);
        assert_eq!(
            merged,
            vec![
                PathBuf::from("/music/album"),
                PathBuf::from("/music/other.flac")
            ]
        );
    }

    #[test]
    fn retry_wait_doubles_until_thirty_seconds() {
        assert_eq!(next_retry_wait(INITIAL_RETRY), Duration::from_secs(4));
        assert_eq!(next_retry_wait(Duration::from_secs(16)), MAX_RETRY);
        assert_eq!(next_retry_wait(MAX_RETRY), MAX_RETRY);
    }

    #[test]
    fn remount_detection_requires_a_directory() {
        let missing = PathBuf::from("/qingyin-missing-watch-root-should-not-exist");
        assert!(!remounted_failed_roots(&[WatchRootStatus {
            path: missing,
            watching: false,
            error: Some("目录当前不可用".into()),
        }]));

        let available = std::env::temp_dir();
        assert!(remounted_failed_roots(&[WatchRootStatus {
            path: available,
            watching: false,
            error: Some("目录当前不可用".into()),
        }]));
        assert!(!remounted_failed_roots(&[WatchRootStatus {
            path: std::env::temp_dir(),
            watching: true,
            error: None,
        }]));
    }

    #[test]
    fn remounted_root_requests_reconcile_and_stop_cancels_retry() {
        let work = unique_temp("watch-remount");
        let root = work.join("music");
        let database_path = work.join("library.sqlite3");
        Database::open(&database_path).unwrap();

        let (tx, rx) = mpsc::channel();
        let watcher = watch_directories(vec![root.clone()], database_path, move |snapshot| {
            let _ = tx.send(snapshot);
        })
        .unwrap();
        assert!(
            watcher
                .root_status()
                .iter()
                .any(|status| !status.watching && status.path == root)
        );

        fs::create_dir_all(&root).unwrap();
        let snapshot = rx
            .recv_timeout(Duration::from_secs(6))
            .expect("remount should request reconcile");
        assert!(snapshot.summary.needs_reconcile);
        assert!(remounted_failed_roots(&snapshot.summary.roots));

        let started = Instant::now();
        drop(watcher);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "watcher stop should cancel remount retry without an unbounded join"
        );
        let _ = fs::remove_dir_all(&work);
    }

    fn unique_temp(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "qingyin-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
