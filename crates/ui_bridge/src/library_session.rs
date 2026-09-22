use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use qingyin_core::{SortColumn, XdgDirs};
use qingyin_library::{
    CollectionEntry, CoverLookup, CoverPriority, CoverRequest, CoverScheduler, CoverService,
    CoverUpdate, LibraryError, LibraryWatcher, ScanEvent, ScanSummary, TrackSnapshot,
    WatchSnapshot, WatchSummary, aggregate_albums, aggregate_artists, aggregate_directories,
    commit_parsed_cover, normalize_roots, prune_unwatched_tracks, scan_directory_with,
    watch_directories,
};
use qingyin_metadata::TrackMetadata;
use qingyin_storage::{Database, SearchHits, StorageError};
use qmetaobject::QUrl;
use qmetaobject::prelude::*;
use thiserror::Error;
use tracing::warn;

use crate::collections::{CollectionModel, DetailTrackModel, TrackListModel};

const SEARCH_RESULT_LIMIT: usize = 500;
const WORKER_JOIN: Duration = Duration::from_millis(100);
const VISIBLE_COVER_HINT: usize = 64;

#[derive(Debug, Clone)]
struct LibrarySnapshot {
    tracks: Vec<TrackSnapshot>,
    artists: Vec<CollectionEntry>,
    albums: Vec<CollectionEntry>,
    directories: Vec<CollectionEntry>,
    status: String,
    roots_generation: u64,
}

#[derive(Debug, Clone)]
struct SearchSnapshot {
    tracks: Vec<TrackSnapshot>,
    has_more: bool,
    query_generation: u64,
    library_revision: u64,
    roots_generation: u64,
}

#[derive(Debug, Error)]
enum LibraryIoError {
    #[error("unable to determine user data directory")]
    MissingDataHome,
    #[error("failed to create library directory {path}: {source}")]
    CreateDataDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to open library database: {0}")]
    OpenDatabase(#[source] StorageError),
    #[error("failed to list library tracks: {0}")]
    ListTracks(#[source] StorageError),
    #[error("failed to search library tracks: {0}")]
    SearchTracks(#[source] StorageError),
    #[error("failed to prune unwatched tracks: {0}")]
    Prune(#[source] LibraryError),
    #[error("failed to start library restore thread")]
    RestoreSpawn,
}

impl LibraryIoError {
    fn to_user_message(&self) -> String {
        match self {
            Self::MissingDataHome => "无法确定用户数据目录".into(),
            Self::CreateDataDir { .. } => "无法创建曲库目录".into(),
            Self::OpenDatabase(_) => "无法打开曲库".into(),
            Self::ListTracks(_) => "无法读取曲库".into(),
            Self::SearchTracks(_) => "搜索失败".into(),
            Self::Prune(_) => "无法更新曲库".into(),
            Self::RestoreSpawn => "无法启动曲库恢复".into(),
        }
    }
}

#[derive(Debug)]
pub enum HostEvent {
    Persist,
    Play {
        tracks: Vec<TrackSnapshot>,
        track_id: i64,
    },
    TrackRemoved,
}

struct LatestMailbox<T> {
    slot: Mutex<Option<T>>,
    cvar: Condvar,
    stop: AtomicBool,
}

impl<T> LatestMailbox<T> {
    fn new() -> Self {
        Self {
            slot: Mutex::new(None),
            cvar: Condvar::new(),
            stop: AtomicBool::new(false),
        }
    }

    fn send(&self, value: T) {
        *self
            .slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(value);
        self.cvar.notify_one();
    }

    fn recv_timeout(&self, timeout: Duration) -> Option<T> {
        let mut slot = self
            .slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.stop.load(Ordering::Relaxed) {
            return None;
        }
        if slot.is_none() {
            let (guard, _) = self
                .cvar
                .wait_timeout(slot, timeout)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            slot = guard;
        }
        if self.stop.load(Ordering::Relaxed) {
            return None;
        }
        slot.take()
    }

    fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.cvar.notify_all();
    }
}

#[derive(Clone)]
struct SearchJob {
    query: String,
    query_generation: u64,
    library_revision: u64,
    roots_generation: u64,
    roots: Vec<PathBuf>,
}

/// Scan, search, watch, and collection models.
#[allow(missing_debug_implementations, clippy::struct_excessive_bools)]
#[derive(QObject, Default)]
pub struct LibrarySession {
    base: qt_base_class!(trait QObject),
    library_model: qt_property!(RefCell<TrackListModel>; CONST),
    library_tracks: Vec<TrackSnapshot>,
    library_by_id: HashMap<i64, TrackSnapshot>,
    music_directories: Vec<PathBuf>,
    host: Option<Arc<dyn Fn(HostEvent) + Send + Sync>>,
    artist_model: qt_property!(RefCell<CollectionModel>; CONST),
    album_model: qt_property!(RefCell<CollectionModel>; CONST),
    directory_model: qt_property!(RefCell<CollectionModel>; CONST),
    artist_detail: qt_property!(RefCell<DetailTrackModel>; CONST),
    album_detail: qt_property!(RefCell<DetailTrackModel>; CONST),
    directory_detail: qt_property!(RefCell<DetailTrackModel>; CONST),
    selected_artist: qt_property!(QString; NOTIFY collections_changed),
    selected_artist_subtitle: qt_property!(QString; NOTIFY collections_changed),
    selected_artist_cover: qt_property!(QString; NOTIFY collections_changed),
    selected_artist_id: String,
    selected_album: qt_property!(QString; NOTIFY collections_changed),
    selected_album_subtitle: qt_property!(QString; NOTIFY collections_changed),
    selected_album_cover: qt_property!(QString; NOTIFY collections_changed),
    selected_album_id: String,
    selected_directory: qt_property!(QString; NOTIFY collections_changed),
    selected_directory_subtitle: qt_property!(QString; NOTIFY collections_changed),
    selected_directory_cover: qt_property!(QString; NOTIFY collections_changed),
    selected_directory_id: String,
    playing_track_id: i64,
    collections_changed: qt_signal!(),
    track_count: qt_property!(i32; NOTIFY track_count_changed),
    track_count_changed: qt_signal!(),
    search_generation: u64,
    search_query: qt_property!(QString; NOTIFY search_changed),
    searching: qt_property!(bool; NOTIFY search_changed),
    search_status: qt_property!(QString; NOTIFY search_changed),
    search_truncated: qt_property!(bool; NOTIFY search_changed),
    search_changed: qt_signal!(),
    sort_column: SortColumn,
    sort_column_name: qt_property!(QString; NOTIFY sort_changed),
    sort_ascending: qt_property!(bool; NOTIFY sort_changed),
    sort_changed: qt_signal!(),
    watcher: Option<LibraryWatcher>,
    pending_rewatch: bool,
    pending_reconcile: bool,
    roots_generation: u64,
    library_revision: u64,
    scanning: qt_property!(bool; NOTIFY scanning_changed),
    restoring: bool,
    busy: qt_property!(bool; NOTIFY scanning_changed),
    scanning_changed: qt_signal!(),
    scan_status: qt_property!(QString; NOTIFY scan_status_changed),
    watch_status: qt_property!(QString; NOTIFY scan_status_changed),
    scan_status_changed: qt_signal!(),
    cover_service: Option<Arc<CoverService>>,
    cover_scheduler: Option<CoverScheduler>,
    search_mailbox: Option<Arc<LatestMailbox<SearchJob>>>,
    search_worker: Option<JoinHandle<()>>,
    task_lock: Arc<Mutex<()>>,
    shutdown: Arc<AtomicBool>,
    add_library_folder: qt_method!(
        fn add_library_folder(&mut self, folder: QUrl) {
            self.add_library_folder_internal(folder);
        }
    ),
    play_track: qt_method!(
        fn play_track(&mut self, track_id: i64) {
            let tracks = self.library_model.borrow().snapshot();
            self.play_listed_id(&tracks, track_id);
        }
    ),
    search_tracks: qt_method!(
        #[allow(clippy::needless_pass_by_value)]
        fn search_tracks(&mut self, query: QString) {
            self.search_tracks_internal(&query);
        }
    ),
    clear_search: qt_method!(
        fn clear_search(&mut self) {
            self.clear_search_internal();
        }
    ),
    filter_collections: qt_method!(
        fn filter_collections(&mut self, mode: i32, query: QString) {
            let changed = match mode {
                1 => self
                    .artist_model
                    .borrow_mut()
                    .set_filter(&query.to_string()),
                2 => self.album_model.borrow_mut().set_filter(&query.to_string()),
                3 => self
                    .directory_model
                    .borrow_mut()
                    .set_filter(&query.to_string()),
                _ => false,
            };
            if changed {
                match mode {
                    1 => self.close_artist_internal(),
                    2 => self.close_album_internal(),
                    3 => self.close_directory_internal(),
                    _ => {}
                }
            }
        }
    ),
    set_sort: qt_method!(
        #[allow(clippy::needless_pass_by_value)]
        fn set_sort(&mut self, column: QString) {
            self.set_sort_internal(&column);
        }
    ),
    open_artist: qt_method!(
        #[allow(clippy::needless_pass_by_value)]
        fn open_artist(&mut self, collection_id: QString) {
            self.show_artist_by_id(&collection_id.to_string());
        }
    ),
    close_artist: qt_method!(
        fn close_artist(&mut self) {
            self.close_artist_internal();
        }
    ),
    play_artist_track: qt_method!(
        fn play_artist_track(&mut self, track_id: i64) {
            let tracks = self.artist_detail.borrow().snapshot();
            self.play_listed_id(&tracks, track_id);
        }
    ),
    open_album: qt_method!(
        #[allow(clippy::needless_pass_by_value)]
        fn open_album(&mut self, collection_id: QString) {
            self.show_album_by_id(&collection_id.to_string());
        }
    ),
    close_album: qt_method!(
        fn close_album(&mut self) {
            self.close_album_internal();
        }
    ),
    play_album_track: qt_method!(
        fn play_album_track(&mut self, track_id: i64) {
            let tracks = self.album_detail.borrow().snapshot();
            self.play_listed_id(&tracks, track_id);
        }
    ),
    open_directory: qt_method!(
        #[allow(clippy::needless_pass_by_value)]
        fn open_directory(&mut self, collection_id: QString) {
            self.show_directory_by_id(&collection_id.to_string());
        }
    ),
    close_directory: qt_method!(
        fn close_directory(&mut self) {
            self.close_directory_internal();
        }
    ),
    play_directory_track: qt_method!(
        fn play_directory_track(&mut self, track_id: i64) {
            let tracks = self.directory_detail.borrow().snapshot();
            self.play_listed_id(&tracks, track_id);
        }
    ),
}

impl LibrarySession {
    pub fn set_host(&mut self, host: impl Fn(HostEvent) + Send + Sync + 'static) {
        self.host = Some(Arc::new(host));
    }

    #[must_use]
    pub fn music_directories(&self) -> &[PathBuf] {
        &self.music_directories
    }

    #[must_use]
    pub fn sort_column(&self) -> SortColumn {
        self.sort_column
    }

    #[must_use]
    pub const fn sort_is_ascending(&self) -> bool {
        self.sort_ascending
    }

    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(mailbox) = &self.search_mailbox {
            mailbox.stop();
        }
        self.release_watcher(false);
        if let Some(mut scheduler) = self.cover_scheduler.take() {
            scheduler.shutdown();
        }
        if let Some(worker) = self.search_worker.take() {
            join_timeout(worker);
        }
    }

    pub fn restore(
        &mut self,
        directories: Vec<PathBuf>,
        sort_column: SortColumn,
        sort_ascending: bool,
        can_prune: bool,
    ) {
        self.music_directories = normalize_roots(directories);
        self.sort_column = sort_column;
        self.sort_ascending = sort_ascending;
        self.sort_column_name = sort_column.as_str().into();
        self.sort_changed();
        self.roots_generation = self.roots_generation.wrapping_add(1);
        self.set_busy(true, true);
        self.scan_status = "正在恢复曲库…".into();
        self.scan_status_changed();
        let roots = self.music_directories.clone();
        let roots_generation = self.roots_generation;
        let session = QPointer::from(&*self);
        let apply =
            qmetaobject::queued_callback(move |result: Result<LibrarySnapshot, LibraryIoError>| {
                let Some(session) = session.as_pinned() else {
                    return;
                };
                let mut session = session.borrow_mut();
                session.set_busy(false, false);
                match result {
                    Ok(snapshot) => {
                        // Search connections require the schema installed by restore_library.
                        session.ensure_services();
                        session.apply_library_snapshot(snapshot);
                        if !session.music_directories.is_empty() {
                            let directories = session.music_directories.clone();
                            session.scan_directories(directories);
                        }
                        session.ensure_watcher();
                        session.flush_pending_watch_work();
                    }
                    Err(error) => session.apply_scan_failure(&error),
                }
            });
        let shutdown = Arc::clone(&self.shutdown);
        let lock = Arc::clone(&self.task_lock);
        if thread::Builder::new()
            .name("qingyin-restore".into())
            .spawn(move || {
                let _guard = lock.lock();
                if shutdown.load(Ordering::Relaxed) {
                    return;
                }
                apply(restore_library(&roots, can_prune, roots_generation));
            })
            .is_err()
        {
            self.set_busy(false, false);
            self.apply_scan_failure(&LibraryIoError::RestoreSpawn);
        }
    }

    fn ensure_services(&mut self) {
        if self.cover_service.is_none() {
            self.cover_service = CoverService::open_default().ok().map(Arc::new);
        }
        if self.cover_scheduler.is_none()
            && let Some(service) = self.cover_service.clone()
        {
            let session = QPointer::from(&*self);
            let apply = qmetaobject::queued_callback(move |update: CoverUpdate| {
                let Some(session) = session.as_pinned() else {
                    return;
                };
                session.borrow_mut().apply_cover_update(update);
            });
            self.cover_scheduler = Some(CoverScheduler::start(service, apply));
        }
        if self.search_mailbox.is_none() {
            let mailbox = Arc::new(LatestMailbox::new());
            let session = QPointer::from(&*self);
            let apply = qmetaobject::queued_callback(
                move |result: Result<SearchSnapshot, LibraryIoError>| {
                    let Some(session) = session.as_pinned() else {
                        return;
                    };
                    session.borrow_mut().apply_search_snapshot(result);
                },
            );
            let worker_mailbox = Arc::clone(&mailbox);
            let shutdown = Arc::clone(&self.shutdown);
            self.search_worker = thread::Builder::new()
                .name("qingyin-search".into())
                .spawn(move || run_search_worker(worker_mailbox, shutdown, apply))
                .ok();
            self.search_mailbox = Some(mailbox);
        }
    }

    fn emit_host(&self, event: HostEvent) {
        if let Some(host) = &self.host {
            host(event);
        }
    }

    fn set_busy(&mut self, scanning: bool, restoring: bool) {
        self.scanning = scanning;
        self.restoring = restoring;
        self.busy = scanning || restoring;
        self.scanning_changed();
    }

    fn add_library_folder_internal(&mut self, folder: QUrl) {
        if self.busy {
            return;
        }
        let folder: String = QString::from(folder).into();
        let Ok(path) = url::Url::parse(&folder).and_then(|url| {
            url.to_file_path()
                .map_err(|()| url::ParseError::RelativeUrlWithoutBase)
        }) else {
            self.scan_status = "选择的文件夹路径无效".into();
            self.scan_status_changed();
            return;
        };
        let path = normalize_roots(vec![path])
            .into_iter()
            .next()
            .expect("folder should normalize to a root");
        if self.add_music_directory(path.clone()) {
            self.emit_host(HostEvent::Persist);
        }
        self.roots_generation = self.roots_generation.wrapping_add(1);
        self.scan_directories(vec![path]);
        self.ensure_watcher();
    }

    fn scan_directories(&mut self, directories: Vec<PathBuf>) {
        if self.busy || directories.is_empty() {
            return;
        }
        self.set_busy(true, false);
        self.scan_status = "正在扫描音乐文件…".into();
        self.scan_status_changed();
        let roots = self.music_directories.clone();
        let roots_generation = self.roots_generation;
        let session = QPointer::from(&*self);
        let apply =
            qmetaobject::queued_callback(move |result: Result<LibrarySnapshot, LibraryIoError>| {
                let Some(session) = session.as_pinned() else {
                    return;
                };
                let mut session = session.borrow_mut();
                session.set_busy(false, false);
                match result {
                    Ok(snapshot) => session.apply_library_snapshot(snapshot),
                    Err(error) => session.apply_scan_failure(&error),
                }
                session.flush_pending_watch_work();
            });
        let shutdown = Arc::clone(&self.shutdown);
        let lock = Arc::clone(&self.task_lock);
        let _ = thread::Builder::new()
            .name("qingyin-scan".into())
            .spawn(move || {
                let _guard = lock.lock();
                if shutdown.load(Ordering::Relaxed) {
                    return;
                }
                apply(scan_library(&directories, &roots, roots_generation));
            });
    }

    fn apply_library_snapshot(&mut self, snapshot: LibrarySnapshot) {
        let _span = tracing::info_span!(
            "ui.apply_snapshot",
            tracks = snapshot.tracks.len(),
            roots_generation = snapshot.roots_generation
        )
        .entered();
        if snapshot.roots_generation != self.roots_generation {
            self.request_latest_snapshot();
            return;
        }
        self.library_revision = self.library_revision.wrapping_add(1);
        self.library_tracks = snapshot.tracks;
        self.library_by_id = self
            .library_tracks
            .iter()
            .cloned()
            .map(|track| (track.id(), track))
            .collect();
        self.publish_track_count();
        if self.search_query.is_empty() {
            self.show_library_tracks();
        } else {
            self.rerun_search();
        }
        self.apply_collections(snapshot.artists, snapshot.albums, snapshot.directories);
        self.scan_status = snapshot.status.into();
        self.scan_status_changed();
        self.queue_missing_covers();
    }

    fn apply_search_snapshot(&mut self, result: Result<SearchSnapshot, LibraryIoError>) {
        match result {
            Ok(snapshot) => {
                if snapshot.query_generation != self.search_generation
                    || snapshot.library_revision != self.library_revision
                    || snapshot.roots_generation != self.roots_generation
                {
                    return;
                }
                self.searching = false;
                let count = snapshot.tracks.len();
                self.replace_visible_tracks(snapshot.tracks, true);
                self.search_truncated = snapshot.has_more;
                self.search_status = if snapshot.has_more {
                    format!("找到 {count} 首歌曲（结果已截断）").into()
                } else {
                    format!("找到 {count} 首歌曲").into()
                };
                self.search_changed();
            }
            Err(error) => {
                self.searching = false;
                self.search_status = error.to_user_message().into();
                self.search_changed();
            }
        }
    }

    fn apply_cover_update(&mut self, update: CoverUpdate) {
        if let Some(track) = self.library_by_id.get_mut(&update.track_id) {
            track.cover_url = update.url.clone();
        }
        if let Some(track) = self
            .library_tracks
            .iter_mut()
            .find(|track| track.id() == update.track_id)
        {
            track.cover_url = update.url.clone();
        }
        self.library_model
            .borrow_mut()
            .set_cover(update.track_id, update.url.clone());
        self.artist_detail
            .borrow_mut()
            .set_cover(update.track_id, update.url.clone());
        self.album_detail
            .borrow_mut()
            .set_cover(update.track_id, update.url.clone());
        self.directory_detail
            .borrow_mut()
            .set_cover(update.track_id, update.url);
    }

    fn apply_watch_snapshot(&mut self, snapshot: WatchSnapshot) {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
        let WatchSnapshot {
            summary,
            tracks,
            artists,
            albums,
            directories,
        } = snapshot;
        let remounted = summary.needs_reconcile
            && !summary.overflow
            && summary.roots.iter().any(|root| !root.watching);
        if summary.overflow {
            self.pending_reconcile = true;
            self.watch_status = "监听事件丢失，已请求重新对账".into();
            self.scan_status_changed();
        } else if remounted {
            self.pending_rewatch = true;
            self.pending_reconcile = true;
            self.watch_status = "目录已重新挂载，正在恢复监听".into();
            self.scan_status_changed();
        } else if summary.all_roots_failed() {
            self.watch_status = "目录监听全部失败".into();
            self.scan_status_changed();
        } else if summary.roots.iter().any(|root| !root.watching) {
            self.watch_status = "部分目录监听失败".into();
            self.scan_status_changed();
        }
        if !summary.has_library_changes() {
            if summary.failed > 0 {
                self.scan_status = format!("曲库更新失败 {} 项", summary.failed).into();
                self.scan_status_changed();
            }
            self.flush_pending_watch_work();
            return;
        }
        let id_changes = summary.upserted_ids.len() + summary.removed_ids.len();
        let incremental = !summary.overflow
            && !summary.needs_reconcile
            && id_changes > 0
            && id_changes <= 8
            && id_changes == summary.upserted + summary.removed;
        if incremental {
            for id in &summary.removed_ids {
                self.library_by_id.remove(id);
                self.library_tracks.retain(|track| track.id() != *id);
                self.library_model.borrow_mut().remove_ids(&[*id]);
            }
            for id in &summary.upserted_ids {
                if let Some(track) = tracks.iter().find(|track| track.id() == *id).cloned() {
                    self.library_by_id.insert(*id, track.clone());
                    if let Some(existing) =
                        self.library_tracks.iter_mut().find(|item| item.id() == *id)
                    {
                        *existing = track.clone();
                    } else {
                        self.library_tracks.push(track.clone());
                    }
                    self.library_model.borrow_mut().upsert(track);
                }
            }
            self.publish_track_count();
            self.apply_collections(artists, albums, directories);
            self.library_revision = self.library_revision.wrapping_add(1);
            self.queue_missing_covers();
        } else {
            self.apply_library_snapshot(LibrarySnapshot {
                tracks,
                artists,
                albums,
                directories,
                status: watch_status(&summary),
                roots_generation: self.roots_generation,
            });
        }
        if !self.search_query.is_empty() {
            self.rerun_search();
        }
        if !summary.removed_ids.is_empty() || summary.overflow {
            self.emit_host(HostEvent::TrackRemoved);
        }
        if !self.busy {
            self.scan_status = watch_status(&summary).into();
            self.scan_status_changed();
        }
        self.flush_pending_watch_work();
    }

    fn request_latest_snapshot(&mut self) {
        if self.busy {
            return;
        }
        let roots = self.music_directories.clone();
        self.scan_directories(roots);
    }

    fn queue_missing_covers(&mut self) {
        let Some(scheduler) = self.cover_scheduler.as_ref() else {
            return;
        };
        let playing = self.playing_track_id;
        let mut protected = Vec::new();
        if let Some(digest) = self
            .library_by_id
            .get(&playing)
            .and_then(|track| track.metadata.cover_digest.clone())
        {
            protected.push(digest);
        }
        scheduler.protect(protected);
        let mut tracks = self.library_tracks.clone();
        crate::sort_snapshots(&mut tracks, self.sort_column, self.sort_ascending);
        for (index, track) in tracks.iter().enumerate() {
            if !track.cover_url.is_empty() {
                continue;
            }
            let priority = if track.id() == playing {
                CoverPriority::Playing
            } else if index < VISIBLE_COVER_HINT {
                CoverPriority::Visible
            } else {
                CoverPriority::Background
            };
            scheduler.request(CoverRequest {
                track_id: track.id(),
                path: track.metadata.path.clone(),
                fingerprint: track.metadata.fingerprint(),
                cover_digest: track.metadata.cover_digest.clone(),
                priority,
            });
        }
    }

    fn search_tracks_internal(&mut self, query: &QString) {
        let query = query.to_string().trim().to_owned();
        if query.is_empty() {
            self.clear_search_internal();
            return;
        }
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_query = query.clone().into();
        self.searching = true;
        self.search_truncated = false;
        self.search_status = "正在搜索…".into();
        self.search_changed();
        if let Some(mailbox) = &self.search_mailbox {
            mailbox.send(SearchJob {
                query,
                query_generation: self.search_generation,
                library_revision: self.library_revision,
                roots_generation: self.roots_generation,
                roots: self.music_directories.clone(),
            });
        }
    }

    fn rerun_search(&mut self) {
        let query = self.search_query.clone();
        if !query.is_empty() {
            self.search_tracks_internal(&query);
        }
    }

    fn clear_search_internal(&mut self) {
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_query = QString::default();
        self.searching = false;
        self.search_truncated = false;
        self.search_status = QString::default();
        self.show_library_tracks();
        self.search_changed();
    }

    fn set_sort_internal(&mut self, column: &QString) {
        let Some(column) = SortColumn::from_name(&column.to_string()) else {
            return;
        };
        if self.sort_column == column {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_column = column;
            self.sort_ascending = true;
        }
        self.sort_column_name = self.sort_column.as_str().into();
        self.emit_host(HostEvent::Persist);
        self.library_model
            .borrow_mut()
            .sort_in_place(self.sort_column, self.sort_ascending);
        self.sort_changed();
    }

    fn flush_pending_watch_work(&mut self) {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
        if self.pending_rewatch {
            self.pending_rewatch = false;
            self.ensure_watcher();
        }
        if self.pending_reconcile && !self.busy {
            self.pending_reconcile = false;
            self.request_latest_snapshot();
        }
    }

    fn ensure_watcher(&mut self) {
        self.release_watcher(false);
        let directories = self.music_directories.clone();
        if directories.is_empty() {
            self.watch_status = QString::default();
            self.scan_status_changed();
            return;
        }
        let Ok(database_path) = database_path() else {
            return;
        };
        let session = QPointer::from(&*self);
        let apply = qmetaobject::queued_callback(move |snapshot: WatchSnapshot| {
            let Some(session) = session.as_pinned() else {
                return;
            };
            session.borrow_mut().apply_watch_snapshot(snapshot);
        });
        match watch_directories(directories, database_path, apply) {
            Ok(watcher) => {
                if watcher.root_status().iter().all(|root| !root.watching)
                    && !watcher.root_status().is_empty()
                {
                    self.watch_status = "目录监听失败".into();
                } else if watcher.root_status().iter().any(|root| !root.watching) {
                    self.watch_status = "部分目录监听失败".into();
                } else {
                    self.watch_status = QString::default();
                }
                self.scan_status_changed();
                self.watcher = Some(watcher);
            }
            Err(error) => {
                warn!(%error, "failed to watch music directories");
                self.watch_status = "目录监听失败".into();
                self.scan_status_changed();
            }
        }
    }

    fn apply_scan_failure(&mut self, error: &LibraryIoError) {
        warn!(%error, "library scan failed");
        self.scan_status = error.to_user_message().into();
        self.scan_status_changed();
    }

    fn play_listed_id(&mut self, tracks: &[TrackSnapshot], track_id: i64) {
        self.playing_track_id = track_id;
        if let Some(scheduler) = &self.cover_scheduler
            && let Some(track) = tracks.iter().find(|track| track.id() == track_id)
        {
            scheduler.request(CoverRequest {
                track_id,
                path: track.metadata.path.clone(),
                fingerprint: track.metadata.fingerprint(),
                cover_digest: track.metadata.cover_digest.clone(),
                priority: CoverPriority::Playing,
            });
            if let Some(digest) = track.metadata.cover_digest.clone() {
                scheduler.protect(vec![digest]);
            }
        }
        self.emit_host(HostEvent::Play {
            tracks: tracks.to_vec(),
            track_id,
        });
    }

    fn add_music_directory(&mut self, path: PathBuf) -> bool {
        if self
            .music_directories
            .iter()
            .any(|existing| existing == &path)
        {
            return false;
        }
        self.music_directories.push(path);
        self.music_directories = normalize_roots(std::mem::take(&mut self.music_directories));
        true
    }

    fn show_library_tracks(&mut self) {
        self.replace_visible_tracks(self.library_tracks.clone(), true);
    }

    fn publish_track_count(&mut self) {
        let count = i32::try_from(self.library_tracks.len()).unwrap_or(i32::MAX);
        if self.track_count == count {
            return;
        }
        self.track_count = count;
        self.track_count_changed();
    }

    fn replace_visible_tracks(&mut self, mut tracks: Vec<TrackSnapshot>, apply_sort: bool) {
        if apply_sort {
            crate::sort_snapshots(&mut tracks, self.sort_column, self.sort_ascending);
        }
        self.library_model.borrow_mut().replace(tracks);
    }

    fn apply_collections(
        &mut self,
        artists: Vec<CollectionEntry>,
        albums: Vec<CollectionEntry>,
        directories: Vec<CollectionEntry>,
    ) {
        let selected_artist = self.selected_artist_id.clone();
        let selected_album = self.selected_album_id.clone();
        let selected_directory = self.selected_directory_id.clone();
        self.artist_model.borrow_mut().reset(artists);
        self.album_model.borrow_mut().reset(albums);
        self.directory_model.borrow_mut().reset(directories);
        if selected_artist.is_empty() {
            self.close_artist_internal();
        } else {
            self.show_artist_by_id(&selected_artist);
        }
        if selected_album.is_empty() {
            self.close_album_internal();
        } else {
            self.show_album_by_id(&selected_album);
        }
        self.show_directory_by_id(&selected_directory);
        self.collections_changed();
    }

    fn release_watcher(&mut self, wait: bool) {
        let Some(watcher) = self.watcher.take() else {
            return;
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let _ = thread::Builder::new()
            .name("qingyin-watch-stop".into())
            .spawn(move || {
                drop(watcher);
                let _ = tx.send(());
            });
        if wait {
            let _ = rx.recv_timeout(WORKER_JOIN);
        }
    }

    fn close_artist_internal(&mut self) {
        if self.selected_artist_id.is_empty() {
            return;
        }
        self.selected_artist = QString::default();
        self.selected_artist_subtitle = QString::default();
        self.selected_artist_cover = QString::default();
        self.selected_artist_id.clear();
        self.artist_detail.borrow_mut().reset(Vec::new());
        self.collections_changed();
    }

    fn show_artist_by_id(&mut self, id: &str) {
        let entry = self.artist_model.borrow().entry_by_id(id);
        match entry {
            Some(entry) => self.show_artist_entry(entry),
            None => self.close_artist_internal(),
        }
    }

    fn show_artist_entry(&mut self, entry: CollectionEntry) {
        self.selected_artist_id = entry.id.clone();
        self.selected_artist = entry.name.into();
        self.selected_artist_subtitle = entry.subtitle.into();
        self.selected_artist_cover = entry.cover_url.into();
        self.artist_detail.borrow_mut().reset(entry.tracks);
        self.collections_changed();
    }

    fn close_album_internal(&mut self) {
        if self.selected_album_id.is_empty() {
            return;
        }
        self.selected_album = QString::default();
        self.selected_album_subtitle = QString::default();
        self.selected_album_cover = QString::default();
        self.selected_album_id.clear();
        self.album_detail.borrow_mut().reset(Vec::new());
        self.collections_changed();
    }

    fn show_album_by_id(&mut self, id: &str) {
        let entry = self.album_model.borrow().entry_by_id(id);
        match entry {
            Some(entry) => self.show_album_entry(entry),
            None => self.close_album_internal(),
        }
    }

    fn show_album_entry(&mut self, entry: CollectionEntry) {
        self.selected_album_id = entry.id.clone();
        self.selected_album = entry.name.into();
        self.selected_album_subtitle = entry.subtitle.into();
        self.selected_album_cover = entry.cover_url.into();
        self.album_detail.borrow_mut().reset(entry.tracks);
        self.collections_changed();
    }
    fn close_directory_internal(&mut self) {
        if self.selected_directory_id.is_empty() {
            return;
        }
        self.selected_directory = QString::default();
        self.selected_directory_subtitle = QString::default();
        self.selected_directory_cover = QString::default();
        self.selected_directory_id.clear();
        self.directory_detail.borrow_mut().reset(Vec::new());
        self.collections_changed();
    }

    fn show_directory_by_id(&mut self, id: &str) {
        let entry = self.directory_model.borrow().entry_by_id(id);
        match entry {
            Some(entry) => self.show_directory_entry(entry),
            None => self.close_directory_internal(),
        }
    }

    fn show_directory_entry(&mut self, entry: CollectionEntry) {
        self.selected_directory_id = entry.id.clone();
        self.selected_directory = entry.name.into();
        self.selected_directory_subtitle = entry.subtitle.into();
        self.selected_directory_cover = entry.cover_url.into();
        self.directory_detail.borrow_mut().reset(entry.tracks);
        self.collections_changed();
    }
}

fn join_timeout(worker: JoinHandle<()>) {
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = worker.join();
        let _ = tx.send(());
    });
    let _ = rx.recv_timeout(WORKER_JOIN);
}

fn run_search_worker(
    mailbox: Arc<LatestMailbox<SearchJob>>,
    shutdown: Arc<AtomicBool>,
    apply: impl Fn(Result<SearchSnapshot, LibraryIoError>),
) {
    let Ok(path) = database_path() else {
        return;
    };
    let mut database = match Database::open_migrated(&path) {
        Ok(database) => database,
        Err(error) => {
            apply(Err(LibraryIoError::OpenDatabase(error)));
            return;
        }
    };
    let covers = CoverService::open_default().ok();
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        let Some(job) = mailbox.recv_timeout(Duration::from_millis(200)) else {
            continue;
        };
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        let result = search_with(&mut database, covers.as_ref(), job);
        apply(result);
    }
}

fn search_with(
    database: &mut Database,
    covers: Option<&CoverService>,
    job: SearchJob,
) -> Result<SearchSnapshot, LibraryIoError> {
    let hits: SearchHits = database
        .search_tracks(&job.query, SEARCH_RESULT_LIMIT, &job.roots)
        .map_err(LibraryIoError::SearchTracks)?;
    let tracks = hits
        .tracks
        .into_iter()
        .map(|stored| snapshot_from_stored(stored.metadata, covers))
        .collect();
    Ok(SearchSnapshot {
        tracks,
        has_more: hits.has_more,
        query_generation: job.query_generation,
        library_revision: job.library_revision,
        roots_generation: job.roots_generation,
    })
}

fn restore_library(
    roots: &[PathBuf],
    can_prune: bool,
    roots_generation: u64,
) -> Result<LibrarySnapshot, LibraryIoError> {
    let mut database = Database::open(database_path()?).map_err(LibraryIoError::OpenDatabase)?;
    if can_prune {
        prune_unwatched_tracks(&mut database, roots).map_err(LibraryIoError::Prune)?;
    }
    load_snapshot(&database, "已恢复", roots, roots_generation)
}

fn scan_library(
    directories: &[PathBuf],
    roots: &[PathBuf],
    roots_generation: u64,
) -> Result<LibrarySnapshot, LibraryIoError> {
    let mut database =
        Database::open_migrated(database_path()?).map_err(LibraryIoError::OpenDatabase)?;
    let covers = CoverService::open_default().ok();
    let mut imported = 0;
    let mut unchanged = 0;
    let mut failed = 0;
    for directory in directories {
        match scan_directory_with(&mut database, directory, |event| match event {
            ScanEvent::Imported { track, cover } => {
                if let Some(service) = &covers {
                    let mut track = track.clone();
                    let _ = commit_parsed_cover(service, &mut track, cover);
                }
            }
            ScanEvent::Unchanged { .. } => {}
        }) {
            Ok(ScanSummary {
                imported: next_imported,
                unchanged: next_unchanged,
                failed: next_failed,
                ..
            }) => {
                imported += next_imported;
                unchanged += next_unchanged;
                failed += next_failed.len();
            }
            Err(error) => {
                warn!(path = %directory.display(), %error, "failed to scan music directory");
                failed += 1;
            }
        }
    }
    prune_unwatched_tracks(&mut database, roots).map_err(LibraryIoError::Prune)?;
    let mut snapshot = load_snapshot(
        &database,
        &format!("扫描完成：导入 {imported} 首，跳过 {unchanged} 首，失败 {failed} 首"),
        roots,
        roots_generation,
    )?;
    if let Some(service) = &covers {
        for track in &mut snapshot.tracks {
            if let CoverLookup::Ready(url) = service.lookup(&track.metadata) {
                track.cover_url = url;
            }
        }
        snapshot.artists = aggregate_artists(&snapshot.tracks);
        snapshot.albums = aggregate_albums(&snapshot.tracks);
        snapshot.directories = aggregate_directories(&snapshot.tracks, roots);
    }
    Ok(snapshot)
}

fn load_snapshot(
    database: &Database,
    status: &str,
    roots: &[PathBuf],
    roots_generation: u64,
) -> Result<LibrarySnapshot, LibraryIoError> {
    let covers = CoverService::open_default().ok();
    let tracks = database
        .list_tracks()
        .map_err(LibraryIoError::ListTracks)?
        .into_iter()
        .map(|stored| snapshot_from_stored(stored.metadata, covers.as_ref()))
        .collect::<Vec<_>>();
    let status = if tracks.is_empty() && status.starts_with("已恢复") {
        String::new()
    } else if status == "已恢复" {
        format!("已恢复 {} 首歌曲", tracks.len())
    } else {
        status.to_owned()
    };
    let artists = aggregate_artists(&tracks);
    let albums = aggregate_albums(&tracks);
    let directories = aggregate_directories(&tracks, roots);
    Ok(LibrarySnapshot {
        tracks,
        artists,
        albums,
        directories,
        status,
        roots_generation,
    })
}

fn snapshot_from_stored(metadata: TrackMetadata, covers: Option<&CoverService>) -> TrackSnapshot {
    let cover_url = covers
        .and_then(|service| match service.lookup(&metadata) {
            CoverLookup::Ready(url) => Some(url),
            _ => None,
        })
        .unwrap_or_default();
    TrackSnapshot::from_shared(Arc::new(metadata), cover_url)
}

fn watch_status(summary: &WatchSummary) -> String {
    if summary.overflow {
        return "监听事件丢失，正在重新对账".into();
    }
    let mut status = format!(
        "已更新曲库：新增 {}，删除 {}",
        summary.upserted, summary.removed
    );
    if summary.failed > 0 {
        let _ = write!(status, "，失败 {}", summary.failed);
    }
    if summary.needs_reconcile {
        let _ = write!(status, "，正在合并对账");
    }
    status
}

fn database_path() -> Result<PathBuf, LibraryIoError> {
    let dirs = XdgDirs::resolve().map_err(|_| LibraryIoError::MissingDataHome)?;
    std::fs::create_dir_all(&dirs.data).map_err(|source| LibraryIoError::CreateDataDir {
        path: dirs.data.clone(),
        source,
    })?;
    Ok(dirs.database_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_detail_plays_its_queue_and_closes_when_removed() {
        let tracks = [(1, "/music/one/a.flac"), (2, "/music/two/b.flac")]
            .into_iter()
            .map(|(id, path)| {
                let mut metadata = TrackMetadata::from_display(
                    PathBuf::from(path),
                    "Song",
                    None,
                    Vec::new(),
                    Some(Duration::from_secs(10)),
                );
                metadata.id = id;
                TrackSnapshot::from_metadata(metadata)
            })
            .collect::<Vec<_>>();
        let mut session = LibrarySession::default();
        let (tx, rx) = mpsc::channel();
        session.set_host(move |event| {
            tx.send(event).unwrap();
        });
        let roots = [PathBuf::from("/music/one"), PathBuf::from("/music/two")];
        session.apply_collections(
            Vec::new(),
            Vec::new(),
            aggregate_directories(&tracks, &roots),
        );
        session.open_directory("directory:/music/one".into());
        assert_eq!(session.selected_directory.to_string(), "one");
        session.play_directory_track(1);
        let HostEvent::Play {
            tracks: queue,
            track_id,
        } = rx.recv().unwrap()
        else {
            panic!("expected directory playback");
        };
        assert_eq!(track_id, 1);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id(), 1);
        session.apply_collections(
            Vec::new(),
            Vec::new(),
            aggregate_directories(&tracks[1..], &roots),
        );
        assert!(session.selected_directory.is_empty());
        assert!(session.directory_detail.borrow().snapshot().is_empty());
    }

    #[test]
    fn scan_and_search_errors_are_matchable_with_chinese_messages() {
        let scan = LibraryIoError::OpenDatabase(StorageError::DurationOverflow);
        let search = LibraryIoError::SearchTracks(StorageError::DurationOverflow);
        assert_eq!(scan.to_user_message(), "无法打开曲库");
        assert_eq!(search.to_user_message(), "搜索失败");
        assert_eq!(
            LibraryIoError::MissingDataHome.to_user_message(),
            "无法确定用户数据目录"
        );
    }

    #[test]
    fn latest_mailbox_keeps_only_the_newest_job() {
        let mailbox = LatestMailbox::new();
        mailbox.send(1);
        mailbox.send(2);
        assert_eq!(mailbox.recv_timeout(Duration::from_millis(10)), Some(2));
    }

    #[test]
    fn track_count_follows_library_tracks() {
        let mut session = LibrarySession::default();
        assert_eq!(session.track_count, 0);
        let mut metadata = TrackMetadata::from_display(
            PathBuf::from("/music/a.flac"),
            "Song",
            None,
            Vec::new(),
            None,
        );
        metadata.id = 1;
        session.library_tracks = vec![TrackSnapshot::from_metadata(metadata)];
        session.publish_track_count();
        assert_eq!(session.track_count, 1);
        session.library_tracks.clear();
        session.publish_track_count();
        assert_eq!(session.track_count, 0);
    }

    #[test]
    fn watch_status_includes_failed_counts() {
        let summary = WatchSummary {
            upserted: 2,
            removed: 1,
            failed: 3,
            ..WatchSummary::default()
        };
        assert_eq!(watch_status(&summary), "已更新曲库：新增 2，删除 1，失败 3");
    }

    #[test]
    fn watch_status_marks_overflow_for_rescan() {
        let summary = WatchSummary {
            overflow: true,
            needs_reconcile: true,
            ..WatchSummary::default()
        };
        assert_eq!(watch_status(&summary), "监听事件丢失，正在重新对账");
    }

    #[test]
    fn ignores_duplicate_music_directories() {
        let mut session = LibrarySession::default();
        assert!(session.add_music_directory(PathBuf::from("/music")));
        assert!(!session.add_music_directory(PathBuf::from("/music")));
        assert_eq!(session.music_directories(), &[PathBuf::from("/music")]);
    }
}
