use std::collections::BTreeSet;
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::event::ModifyKind;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::warn;

use crate::{LibraryChange, LibraryError, refresh_watched_path};
use qingyin_storage::Database;

const DEBOUNCE: Duration = Duration::from_millis(400);

/// Incremental library changes collected from a debounced watcher batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchSummary {
    pub upserted: usize,
    pub removed: usize,
    pub failed: usize,
}

impl WatchSummary {
    #[must_use]
    pub const fn has_library_changes(&self) -> bool {
        self.upserted > 0 || self.removed > 0
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.upserted == 0 && self.removed == 0 && self.failed == 0
    }
}

enum WatchMessage {
    Paths(Vec<PathBuf>),
    Stop,
}

/// Recursively watches music directories and refreshes changed paths off the UI thread.
pub struct LibraryWatcher {
    command_tx: Sender<WatchMessage>,
    worker: Option<JoinHandle<()>>,
    _watcher: RecommendedWatcher,
}

impl Debug for LibraryWatcher {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LibraryWatcher")
            .finish_non_exhaustive()
    }
}

impl Drop for LibraryWatcher {
    fn drop(&mut self) {
        let _ = self.command_tx.send(WatchMessage::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Starts a recursive watcher for `directories`.
///
/// Filesystem events are coalesced for [`DEBOUNCE`], then applied with
/// [`MusicLibrary::refresh_path`] on a worker thread that owns its SQLite connection.
///
/// # Errors
///
/// Returns [`LibraryError`] when the native watcher cannot be created.
pub fn watch_directories(
    directories: Vec<PathBuf>,
    database_path: PathBuf,
    on_batch: impl Fn(WatchSummary) + Send + 'static,
) -> Result<LibraryWatcher, LibraryError> {
    let roots = unique_roots(directories);
    let (command_tx, command_rx) = mpsc::channel();
    let event_tx = command_tx.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<Event>| match result {
            Ok(event) => {
                let paths = event_paths(&event);
                if !paths.is_empty() {
                    let _ = event_tx.send(WatchMessage::Paths(paths));
                }
            }
            Err(error) => warn!(%error, "library watcher error"),
        },
        Config::default(),
    )?;

    for directory in &roots {
        if !directory.is_dir() {
            continue;
        }
        if let Err(error) = watcher.watch(directory, RecursiveMode::Recursive) {
            warn!(
                path = %directory.display(),
                %error,
                "failed to watch music directory"
            );
        }
    }

    let worker_roots = roots;
    let worker = thread::spawn(move || {
        let mut pending = BTreeSet::new();
        let mut deadline: Option<Instant> = None;
        loop {
            let received = match deadline {
                Some(until) => {
                    match command_rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
                        Ok(message) => Some(message),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
                None => match command_rx.recv() {
                    Ok(message) => Some(message),
                    Err(_) => break,
                },
            };

            match received {
                Some(WatchMessage::Stop) => break,
                Some(WatchMessage::Paths(paths)) => {
                    pending.extend(paths);
                    deadline = Some(Instant::now() + DEBOUNCE);
                }
                None => {
                    let summary = apply_pending(&database_path, &worker_roots, &mut pending);
                    if !summary.is_empty() {
                        on_batch(summary);
                    }
                    deadline = None;
                }
            }
        }
    });

    Ok(LibraryWatcher {
        command_tx,
        worker: Some(worker),
        _watcher: watcher,
    })
}

fn apply_pending(
    database_path: &Path,
    roots: &[PathBuf],
    pending: &mut BTreeSet<PathBuf>,
) -> WatchSummary {
    let paths = std::mem::take(pending).into_iter().collect::<Vec<_>>();
    if paths.is_empty() {
        return WatchSummary::default();
    }
    apply_paths(database_path, roots, &paths)
}

fn apply_paths(database_path: &Path, roots: &[PathBuf], paths: &[PathBuf]) -> WatchSummary {
    let mut summary = WatchSummary::default();
    let mut database = match Database::open(database_path) {
        Ok(database) => database,
        Err(error) => {
            warn!(%error, "library watcher could not open database");
            summary.failed = summary.failed.saturating_add(1);
            return summary;
        }
    };
    for path in paths {
        match refresh_watched_path(&mut database, path, roots) {
            Ok(LibraryChange::Upserted(count)) => summary.upserted += count,
            Ok(LibraryChange::Removed(count)) => summary.removed += count,
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

fn unique_roots(directories: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for directory in directories {
        let directory = directory.canonicalize().unwrap_or(directory);
        if !roots.iter().any(|existing| existing == &directory) {
            roots.push(directory);
        }
    }
    roots
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
    use super::{event_paths, unique_roots};
    use crate::{is_temporary, is_watched_path};
    use notify::event::{AccessKind, CreateKind, Event, EventKind, ModifyKind};
    use std::path::PathBuf;

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
        let roots = unique_roots(vec![PathBuf::from("/music"), PathBuf::from("/music")]);
        assert_eq!(roots.len(), 1);
    }
}
