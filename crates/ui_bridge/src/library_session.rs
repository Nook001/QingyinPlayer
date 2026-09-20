use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write;
use std::hash::{DefaultHasher, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{ImageFormat, ImageReader, Limits};
use qingyin_library::{
    CollectionEntry, LibraryError, LibraryWatcher, MusicLibrary, ScanEvent, TrackSnapshot,
    WatchSummary, aggregate_albums, aggregate_artists, is_library_track, prune_unwatched_tracks,
    watch_directories,
};
use qingyin_metadata::{CoverArt, TrackMetadata, read_cover};
use qingyin_storage::{Database, StorageError};
use qmetaobject::QUrl;
use qmetaobject::prelude::*;
use thiserror::Error;
use tracing::warn;

use crate::collections::{CollectionModel, DetailTrackModel, TrackListModel};

const MAX_COVER_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_COVER_DECODE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_COVER_SOURCE_EDGE: u32 = 8192;
const MAX_COVER_EDGE: u32 = 512;
const SEARCH_RESULT_LIMIT: usize = 500;

#[derive(Debug)]
struct ScanResult {
    tracks: Vec<TrackSnapshot>,
    cover_urls: Vec<String>,
    artists: Vec<CollectionEntry>,
    albums: Vec<CollectionEntry>,
    status: String,
}

#[derive(Debug)]
struct SearchResult {
    tracks: Vec<TrackMetadata>,
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

type PlayListed = Box<dyn Fn(Vec<TrackMetadata>, Vec<String>, i32)>;

#[derive(Default)]
struct LibraryHost {
    persist: Option<Arc<dyn Fn() + Send + Sync>>,
    current_path: Option<Box<dyn Fn() -> Option<PathBuf>>>,
    set_idle_index: Option<Box<dyn Fn(Option<usize>)>>,
    skip_missing: Option<Box<dyn Fn()>>,
    play_from_list: Option<PlayListed>,
}

/// Scan, search, watch, and collection models. Scan/watch callbacks land here, not on `AppBridge`.
#[allow(missing_debug_implementations, clippy::struct_excessive_bools)]
#[derive(QObject, Default)]
pub struct LibrarySession {
    base: qt_base_class!(trait QObject),
    /// Visible rows bound by the library `TrackTable` (search results or full library).
    library_model: qt_property!(RefCell<TrackListModel>; CONST),
    /// Full imported library, independent of the current search filter.
    library_tracks: Vec<TrackSnapshot>,
    library_cover_urls: Vec<String>,
    music_directories: Vec<PathBuf>,
    host: LibraryHost,
    artist_model: qt_property!(RefCell<CollectionModel>; CONST),
    album_model: qt_property!(RefCell<CollectionModel>; CONST),
    artist_detail: qt_property!(RefCell<DetailTrackModel>; CONST),
    album_detail: qt_property!(RefCell<DetailTrackModel>; CONST),
    selected_artist: qt_property!(QString; NOTIFY collections_changed),
    selected_artist_subtitle: qt_property!(QString; NOTIFY collections_changed),
    selected_artist_cover: qt_property!(QString; NOTIFY collections_changed),
    selected_album: qt_property!(QString; NOTIFY collections_changed),
    selected_album_subtitle: qt_property!(QString; NOTIFY collections_changed),
    selected_album_cover: qt_property!(QString; NOTIFY collections_changed),
    collections_changed: qt_signal!(),
    search_generation: u64,
    search_query: qt_property!(QString; NOTIFY search_changed),
    searching: qt_property!(bool; NOTIFY search_changed),
    search_status: qt_property!(QString; NOTIFY search_changed),
    search_changed: qt_signal!(),
    sort_column: qt_property!(QString; NOTIFY sort_changed),
    sort_ascending: qt_property!(bool; NOTIFY sort_changed),
    sort_changed: qt_signal!(),
    watcher: Option<LibraryWatcher>,
    library_generation: u64,
    scanning: qt_property!(bool; NOTIFY scanning_changed),
    scanning_changed: qt_signal!(),
    scan_status: qt_property!(QString; NOTIFY scan_status_changed),
    scan_status_changed: qt_signal!(),
    add_library_folder: qt_method!(
        fn add_library_folder(&mut self, folder: QUrl) {
            if self.scanning {
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
            let path = normalize_music_directory(path);
            self.scanning = true;
            self.scanning_changed();
            self.scan_status = "正在扫描音乐文件…".into();
            self.scan_status_changed();

            let scan_path = path.clone();
            let mut library_roots = self.music_directories.clone();
            if !library_roots.iter().any(|existing| existing == &scan_path) {
                library_roots.push(scan_path.clone());
            }
            let session = QPointer::from(&*self);
            let apply_result =
                qmetaobject::queued_callback(move |result: Result<ScanResult, LibraryIoError>| {
                    let Some(session) = session.as_pinned() else {
                        return;
                    };
                    let mut session = session.borrow_mut();
                    session.scanning = false;
                    session.scanning_changed();
                    match result {
                        Ok(result) => {
                            if session.add_music_directory(path.clone()) {
                                session.request_persist();
                            }
                            session.apply_scan_result(result);
                            session.ensure_watcher();
                        }
                        Err(error) => session.apply_scan_failure(&error),
                    }
                });

            std::thread::spawn(move || {
                apply_result(scan_music_directory(&scan_path, &library_roots));
            });
        }
    ),
    play_track: qt_method!(
        fn play_track(&mut self, row: i32) {
            let (tracks, covers) = self.library_model.borrow().snapshot();
            self.play_listed(tracks, covers, row);
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
    set_sort: qt_method!(
        #[allow(clippy::needless_pass_by_value)]
        fn set_sort(&mut self, column: QString) {
            self.set_sort_internal(&column);
        }
    ),
    open_artist: qt_method!(
        fn open_artist(&mut self, row: i32) {
            self.open_artist_internal(row);
        }
    ),
    close_artist: qt_method!(
        fn close_artist(&mut self) {
            self.close_artist_internal();
        }
    ),
    play_artist_track: qt_method!(
        fn play_artist_track(&mut self, row: i32) {
            let (tracks, covers) = self.artist_detail.borrow().snapshot();
            self.play_listed(tracks, covers, row);
        }
    ),
    open_album: qt_method!(
        fn open_album(&mut self, row: i32) {
            self.open_album_internal(row);
        }
    ),
    close_album: qt_method!(
        fn close_album(&mut self) {
            self.close_album_internal();
        }
    ),
    play_album_track: qt_method!(
        fn play_album_track(&mut self, row: i32) {
            let (tracks, covers) = self.album_detail.borrow().snapshot();
            self.play_listed(tracks, covers, row);
        }
    ),
}

impl LibrarySession {
    pub fn set_persist(&mut self, persist: impl Fn() + Send + Sync + 'static) {
        self.host.persist = Some(Arc::new(persist));
    }

    pub fn set_current_path(&mut self, current_path: impl Fn() -> Option<PathBuf> + 'static) {
        self.host.current_path = Some(Box::new(current_path));
    }

    pub fn set_idle_index(&mut self, set_idle_index: impl Fn(Option<usize>) + 'static) {
        self.host.set_idle_index = Some(Box::new(set_idle_index));
    }

    pub fn set_skip_missing(&mut self, skip_missing: impl Fn() + 'static) {
        self.host.skip_missing = Some(Box::new(skip_missing));
    }

    pub fn set_play_from_list(
        &mut self,
        play_from_list: impl Fn(Vec<TrackMetadata>, Vec<String>, i32) + 'static,
    ) {
        self.host.play_from_list = Some(Box::new(play_from_list));
    }

    #[must_use]
    pub fn music_directories(&self) -> &[PathBuf] {
        &self.music_directories
    }

    #[must_use]
    pub fn sort_column_string(&self) -> String {
        self.sort_column.to_string()
    }

    #[must_use]
    pub const fn sort_is_ascending(&self) -> bool {
        self.sort_ascending
    }

    pub fn shutdown(&mut self) {
        self.watcher = None;
    }

    pub fn restore(
        &mut self,
        directories: Vec<PathBuf>,
        sort_column: String,
        sort_ascending: bool,
    ) {
        self.music_directories.clone_from(&directories);
        self.sort_column = sort_column.into();
        self.sort_ascending = sort_ascending;
        self.sort_changed();
        self.scan_status = "正在恢复曲库…".into();
        self.scan_status_changed();

        let session = QPointer::from(&*self);
        let apply_result =
            qmetaobject::queued_callback(move |result: Result<ScanResult, LibraryIoError>| {
                let Some(session) = session.as_pinned() else {
                    return;
                };
                let mut session = session.borrow_mut();
                match result {
                    Ok(result) => session.apply_scan_result(result),
                    Err(error) => session.apply_scan_failure(&error),
                }
                let directories = session.music_directories.clone();
                if !directories.is_empty() {
                    session.scan_directories(directories);
                }
                session.ensure_watcher();
            });
        if let Err(error) = std::thread::Builder::new()
            .name("qingyin-restore".into())
            .spawn(move || apply_result(load_stored_library(&directories)))
        {
            warn!(%error, "failed to start library restore");
            self.apply_scan_failure(&LibraryIoError::RestoreSpawn);
        }
    }

    fn request_persist(&self) {
        if let Some(persist) = &self.host.persist {
            persist();
        }
    }

    fn play_listed(&self, tracks: Vec<TrackMetadata>, covers: Vec<String>, row: i32) {
        if let Some(play_from_list) = &self.host.play_from_list {
            play_from_list(tracks, covers, row);
        }
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
        true
    }

    fn apply_scan_failure(&mut self, error: &LibraryIoError) {
        warn!(%error, "library scan failed");
        self.set_scan_error(error.to_user_message());
    }

    fn apply_search_failure(&mut self, error: &LibraryIoError) {
        warn!(%error, "library search failed");
        self.search_status = error.to_user_message().into();
    }

    fn set_scan_error(&mut self, error: String) {
        self.scan_status = error.into();
        self.scan_status_changed();
    }

    fn search_tracks_internal(&mut self, query: &QString) {
        let query = query.to_string().trim().to_owned();
        if query.is_empty() {
            self.clear_search_internal();
            return;
        }

        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;
        self.search_query = query.clone().into();
        self.searching = true;
        self.search_status = "正在搜索…".into();
        self.search_changed();

        let session = QPointer::from(&*self);
        let apply_result =
            qmetaobject::queued_callback(move |result: Result<SearchResult, LibraryIoError>| {
                let Some(session) = session.as_pinned() else {
                    return;
                };
                let mut session = session.borrow_mut();
                if session.search_generation != generation {
                    return;
                }
                session.searching = false;
                match result {
                    Ok(result) => {
                        let count = result.tracks.len();
                        let cover_urls = result
                            .tracks
                            .iter()
                            .map(|track| session.library_cover_for_path(&track.path))
                            .collect();
                        let apply_column_sort = !session.sort_column.is_empty();
                        session.replace_visible_tracks(
                            snapshots_from(result.tracks),
                            cover_urls,
                            apply_column_sort,
                        );
                        session.search_status = format!("找到 {count} 首歌曲").into();
                    }
                    Err(error) => session.apply_search_failure(&error),
                }
                session.search_changed();
            });

        let roots = self.music_directories.clone();
        std::thread::spawn(move || apply_result(search_database(&query, &roots)));
    }

    fn clear_search_internal(&mut self) {
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_query = QString::default();
        self.searching = false;
        self.search_status = QString::default();
        self.show_library_tracks();
        self.search_changed();
    }

    fn set_sort_internal(&mut self, column: &QString) {
        let column = column.to_string();
        if !matches!(column.as_str(), "title" | "album" | "duration") {
            return;
        }
        if self.sort_column == QString::from(column.clone()) {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_column = column.into();
            self.sort_ascending = true;
        }
        self.request_persist();
        self.sort_visible_tracks();
        self.sort_changed();
    }

    fn ensure_watcher(&mut self) {
        self.watcher = None;
        let directories = self.music_directories.clone();
        if directories.is_empty() {
            return;
        }
        let database_path = match database_path() {
            Ok(path) => path,
            Err(error) => {
                warn!(%error, "unable to determine library database path for watcher");
                return;
            }
        };
        let session = QPointer::from(&*self);
        let apply_batch = qmetaobject::queued_callback(move |summary: WatchSummary| {
            let Some(session) = session.as_pinned() else {
                return;
            };
            session.borrow_mut().apply_watch_summary(&summary);
        });
        match watch_directories(directories, database_path, apply_batch) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(error) => warn!(%error, "failed to watch music directories"),
        }
    }

    fn apply_watch_summary(&mut self, summary: &WatchSummary) {
        if !summary.has_library_changes() {
            if summary.failed > 0 {
                self.set_scan_error(format!("曲库更新失败 {} 项", summary.failed));
            }
            return;
        }

        match load_stored_tracks() {
            Ok(tracks) => {
                let tracks = tracks_in_library(tracks, &self.music_directories);
                let cover_urls =
                    reuse_or_cache_covers(&self.library_tracks, &self.library_cover_urls, &tracks);
                let status = watch_status(summary);
                self.library_generation = self.library_generation.wrapping_add(1);
                let generation = self.library_generation;
                let session = QPointer::from(&*self);
                let apply_result = qmetaobject::queued_callback(move |result: ScanResult| {
                    let Some(session) = session.as_pinned() else {
                        return;
                    };
                    let mut session = session.borrow_mut();
                    if session.library_generation != generation {
                        return;
                    }
                    session.apply_scan_result(result);
                    if !session.search_query.is_empty() {
                        let query = session.search_query.clone();
                        session.search_tracks_internal(&query);
                    }
                    if let Some(skip_missing) = &session.host.skip_missing {
                        skip_missing();
                    }
                });
                std::thread::spawn(move || {
                    apply_result(scan_result_with_collections(tracks, cover_urls, status));
                });
            }
            Err(error) => self.apply_scan_failure(&error),
        }
    }

    fn apply_scan_result(&mut self, result: ScanResult) {
        self.library_generation = self.library_generation.wrapping_add(1);
        self.library_tracks = result.tracks;
        self.library_cover_urls = result.cover_urls;
        if self.search_query.is_empty() {
            self.show_library_tracks();
        }
        self.apply_collections(result.artists, result.albums);
        self.scan_status = result.status.into();
        self.scan_status_changed();
    }

    fn scan_directories(&mut self, directories: Vec<PathBuf>) {
        if self.scanning || directories.is_empty() {
            return;
        }
        self.scanning = true;
        self.scanning_changed();
        self.scan_status = "正在核对音乐文件夹…".into();
        self.scan_status_changed();

        let session = QPointer::from(&*self);
        let apply_result =
            qmetaobject::queued_callback(move |result: Result<ScanResult, LibraryIoError>| {
                let Some(session) = session.as_pinned() else {
                    return;
                };
                let mut session = session.borrow_mut();
                session.scanning = false;
                session.scanning_changed();
                match result {
                    Ok(result) => session.apply_scan_result(result),
                    Err(error) => session.apply_scan_failure(&error),
                }
            });
        std::thread::spawn(move || {
            apply_result(scan_music_directories(&directories, &directories));
        });
    }

    fn show_library_tracks(&mut self) {
        self.replace_visible_tracks(
            self.library_tracks.clone(),
            self.library_cover_urls.clone(),
            true,
        );
    }

    fn replace_visible_tracks(
        &mut self,
        tracks: Vec<TrackSnapshot>,
        cover_urls: Vec<String>,
        apply_column_sort: bool,
    ) {
        let current_path = self
            .host
            .current_path
            .as_ref()
            .and_then(|current_path| current_path());
        let mut tracks = tracks;
        let mut cover_urls = cover_urls;
        if apply_column_sort {
            crate::sort_tracks(
                &mut tracks,
                &mut cover_urls,
                &self.sort_column.to_string(),
                self.sort_ascending,
            );
        }
        self.library_model.borrow_mut().replace(tracks, cover_urls);
        let position = current_path
            .as_ref()
            .and_then(|path| self.library_model.borrow().position_of_path(path));
        if let Some(set_idle_index) = &self.host.set_idle_index {
            set_idle_index(position);
        }
    }

    fn sort_visible_tracks(&mut self) {
        self.library_model
            .borrow_mut()
            .sort_in_place(&self.sort_column.to_string(), self.sort_ascending);
    }

    fn library_cover_for_path(&self, path: &Path) -> String {
        self.library_tracks
            .iter()
            .position(|track| track.metadata.path == path)
            .and_then(|index| self.library_cover_urls.get(index))
            .cloned()
            .unwrap_or_default()
    }

    fn apply_collections(&mut self, artists: Vec<CollectionEntry>, albums: Vec<CollectionEntry>) {
        let selected_artist = self.selected_artist.to_string();
        let selected_album = self.selected_album.to_string();
        self.artist_model.borrow_mut().reset(artists);
        self.album_model.borrow_mut().reset(albums);
        if selected_artist.is_empty() {
            self.close_artist_internal();
        } else {
            self.show_artist_by_name(&selected_artist);
        }
        if selected_album.is_empty() {
            self.close_album_internal();
        } else {
            self.show_album_by_name(&selected_album);
        }
        self.collections_changed();
    }

    fn open_artist_internal(&mut self, row: i32) {
        let Some(entry) = usize::try_from(row)
            .ok()
            .and_then(|row| self.artist_model.borrow().entry(row))
        else {
            return;
        };
        self.show_artist_entry(entry);
    }

    fn close_artist_internal(&mut self) {
        self.selected_artist = QString::default();
        self.selected_artist_subtitle = QString::default();
        self.selected_artist_cover = QString::default();
        self.artist_detail
            .borrow_mut()
            .reset(Vec::new(), Vec::new());
        self.collections_changed();
    }

    fn show_artist_by_name(&mut self, name: &str) {
        let entry = self.artist_model.borrow().entry_by_name(name);
        match entry {
            Some(entry) => self.show_artist_entry(entry),
            None => self.close_artist_internal(),
        }
    }

    fn show_artist_entry(&mut self, entry: CollectionEntry) {
        self.selected_artist = entry.name.into();
        self.selected_artist_subtitle = entry.subtitle.into();
        self.selected_artist_cover = entry.cover_url.into();
        self.artist_detail
            .borrow_mut()
            .reset(entry.tracks, entry.cover_urls);
        self.collections_changed();
    }

    fn open_album_internal(&mut self, row: i32) {
        let Some(entry) = usize::try_from(row)
            .ok()
            .and_then(|row| self.album_model.borrow().entry(row))
        else {
            return;
        };
        self.show_album_entry(entry);
    }

    fn close_album_internal(&mut self) {
        self.selected_album = QString::default();
        self.selected_album_subtitle = QString::default();
        self.selected_album_cover = QString::default();
        self.album_detail.borrow_mut().reset(Vec::new(), Vec::new());
        self.collections_changed();
    }

    fn show_album_by_name(&mut self, name: &str) {
        let entry = self.album_model.borrow().entry_by_name(name);
        match entry {
            Some(entry) => self.show_album_entry(entry),
            None => self.close_album_internal(),
        }
    }

    fn show_album_entry(&mut self, entry: CollectionEntry) {
        self.selected_album = entry.name.into();
        self.selected_album_subtitle = entry.subtitle.into();
        self.selected_album_cover = entry.cover_url.into();
        self.album_detail
            .borrow_mut()
            .reset(entry.tracks, entry.cover_urls);
        self.collections_changed();
    }
}

fn scan_music_directory(
    path: &Path,
    library_roots: &[PathBuf],
) -> Result<ScanResult, LibraryIoError> {
    scan_music_directories(&[path.to_path_buf()], library_roots)
}

fn scan_music_directories(
    directories: &[PathBuf],
    library_roots: &[PathBuf],
) -> Result<ScanResult, LibraryIoError> {
    let mut database = Database::open(database_path()?).map_err(LibraryIoError::OpenDatabase)?;
    let mut library = MusicLibrary::default();
    let mut imported = 0;
    let mut unchanged = 0;
    let mut failed = 0;
    for directory in directories {
        match library.scan_directory_with(&mut database, directory, |event| match event {
            ScanEvent::Imported {
                track,
                modified_at,
                cover,
            } => store_cached_cover(&track.path, modified_at, cover.as_ref()),
            ScanEvent::Unchanged { path, modified_at } => ensure_cached_cover(path, modified_at),
        }) {
            Ok(summary) => {
                imported += summary.imported;
                unchanged += summary.unchanged;
                failed += summary.failed.len();
            }
            Err(error) => {
                warn!(path = %directory.display(), %error, "failed to scan music directory");
                failed += 1;
            }
        }
    }
    prune_unwatched_tracks(&database, library_roots).map_err(LibraryIoError::Prune)?;
    let tracks = listed_tracks(&database)?;
    let cover_urls = existing_cover_urls(&tracks, false);
    Ok(scan_result_with_collections(
        tracks,
        cover_urls,
        format!("扫描完成：导入 {imported} 首，跳过 {unchanged} 首，失败 {failed} 首"),
    ))
}

fn load_stored_library(roots: &[PathBuf]) -> Result<ScanResult, LibraryIoError> {
    let database = Database::open(database_path()?).map_err(LibraryIoError::OpenDatabase)?;
    prune_unwatched_tracks(&database, roots).map_err(LibraryIoError::Prune)?;
    let tracks = listed_tracks(&database)?;
    let cover_urls = existing_cover_urls(&tracks, true);
    let status = if tracks.is_empty() {
        String::new()
    } else {
        format!("已恢复 {} 首歌曲", tracks.len())
    };
    Ok(scan_result_with_collections(tracks, cover_urls, status))
}

fn scan_result_with_collections(
    tracks: Vec<TrackSnapshot>,
    cover_urls: Vec<String>,
    status: String,
) -> ScanResult {
    let artists = aggregate_artists(&tracks, &cover_urls);
    let albums = aggregate_albums(&tracks, &cover_urls);
    ScanResult {
        tracks,
        cover_urls,
        artists,
        albums,
        status,
    }
}

fn load_stored_tracks() -> Result<Vec<TrackSnapshot>, LibraryIoError> {
    listed_tracks(&Database::open(database_path()?).map_err(LibraryIoError::OpenDatabase)?)
}

fn listed_tracks(database: &Database) -> Result<Vec<TrackSnapshot>, LibraryIoError> {
    database
        .list_tracks()
        .map_err(LibraryIoError::ListTracks)
        .map(|tracks| {
            snapshots_from(
                tracks
                    .into_iter()
                    .map(|track| track.metadata)
                    .collect::<Vec<_>>(),
            )
        })
}

fn snapshots_from(tracks: Vec<TrackMetadata>) -> Vec<TrackSnapshot> {
    tracks
        .into_iter()
        .map(TrackSnapshot::from_metadata)
        .collect()
}

fn tracks_in_library(tracks: Vec<TrackSnapshot>, roots: &[PathBuf]) -> Vec<TrackSnapshot> {
    tracks
        .into_iter()
        .filter(|track| is_library_track(&track.metadata.path, roots))
        .collect()
}

fn normalize_music_directory(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn reuse_or_cache_covers(
    previous_tracks: &[TrackSnapshot],
    previous_covers: &[String],
    tracks: &[TrackSnapshot],
) -> Vec<String> {
    let mut previous = HashMap::new();
    for (track, cover) in previous_tracks.iter().zip(previous_covers) {
        if !cover.is_empty() {
            previous.insert(
                track.metadata.path.clone(),
                (track.metadata.modified_at, cover.clone()),
            );
        }
    }
    let directory = cache_directory();
    tracks
        .iter()
        .map(|track| {
            if let Some((modified_at, cover)) = previous.get(&track.metadata.path)
                && *modified_at == track.metadata.modified_at
            {
                return cover.clone();
            }
            directory
                .as_ref()
                .and_then(|directory| resolve_cover_url(directory, &track.metadata, true))
                .unwrap_or_default()
        })
        .collect()
}

fn watch_status(summary: &WatchSummary) -> String {
    let mut status = format!(
        "已更新曲库：新增 {}，删除 {}",
        summary.upserted, summary.removed
    );
    if summary.failed > 0 {
        let _ = write!(status, "，失败 {}", summary.failed);
    }
    status
}

fn search_database(query: &str, roots: &[PathBuf]) -> Result<SearchResult, LibraryIoError> {
    let database = Database::open(database_path()?).map_err(LibraryIoError::OpenDatabase)?;
    let tracks = database
        .search_tracks(query, SEARCH_RESULT_LIMIT)
        .map_err(LibraryIoError::SearchTracks)?
        .into_iter()
        .map(|track| track.metadata)
        .filter(|track| is_library_track(&track.path, roots))
        .collect();
    Ok(SearchResult { tracks })
}

fn existing_cover_urls(tracks: &[TrackSnapshot], read_source: bool) -> Vec<String> {
    let Some(directory) = cache_directory() else {
        return vec![String::new(); tracks.len()];
    };
    tracks
        .iter()
        .map(|track| {
            resolve_cover_url(&directory, &track.metadata, read_source).unwrap_or_default()
        })
        .collect()
}

fn resolve_cover_url(directory: &Path, track: &TrackMetadata, read_source: bool) -> Option<String> {
    if let Some(url) = cached_cover_url(directory, &track.path, track.modified_at) {
        return Some(url);
    }
    if !read_source || cover_marked_missing(directory, &track.path, track.modified_at) {
        return None;
    }
    ensure_cached_cover(&track.path, track.modified_at);
    cached_cover_url(directory, &track.path, track.modified_at)
}

fn ensure_cached_cover(track_path: &Path, modified_at: i64) {
    let Some(directory) = cache_directory() else {
        return;
    };
    if cached_cover_url(&directory, track_path, modified_at).is_some()
        || cover_marked_missing(&directory, track_path, modified_at)
    {
        return;
    }
    if let Ok(cover) = read_cover(track_path) {
        store_cached_cover(track_path, modified_at, cover.as_ref());
    }
}

fn store_cached_cover(track_path: &Path, modified_at: i64, cover: Option<&CoverArt>) {
    let Some(directory) = cache_directory() else {
        return;
    };
    let png = cover_cache_file(directory.as_path(), track_path, modified_at, "png");
    let missing = cover_cache_file(directory.as_path(), track_path, modified_at, "missing");
    if let Some(cover) = cover {
        let _ = std::fs::remove_file(&missing);
        if png.exists() {
            return;
        }
        if let Some(bytes) = prepare_cached_cover(&cover.data) {
            let _ = std::fs::write(&png, bytes);
        }
        return;
    }
    let _ = std::fs::remove_file(&png);
    if !missing.exists() {
        let _ = std::fs::write(&missing, []);
    }
}

fn cached_cover_url(directory: &Path, track_path: &Path, modified_at: i64) -> Option<String> {
    let path = cover_cache_file(directory, track_path, modified_at, "png");
    path.exists()
        .then(|| url::Url::from_file_path(path).ok().map(Into::into))
        .flatten()
}

fn cover_marked_missing(directory: &Path, track_path: &Path, modified_at: i64) -> bool {
    cover_cache_file(directory, track_path, modified_at, "missing").exists()
}

fn cover_cache_file(
    directory: &Path,
    track_path: &Path,
    modified_at: i64,
    extension: &str,
) -> PathBuf {
    directory.join(format!(
        "{}.{}",
        cover_cache_stem(track_path, modified_at),
        extension
    ))
}

fn cover_cache_stem(track_path: &Path, modified_at: i64) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(track_path.as_os_str().as_encoded_bytes());
    format!("{:016x}-{modified_at}", hasher.finish())
}

fn prepare_cached_cover(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() > MAX_COVER_SOURCE_BYTES {
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_COVER_SOURCE_EDGE);
    limits.max_image_height = Some(MAX_COVER_SOURCE_EDGE);
    limits.max_alloc = Some(MAX_COVER_DECODE_BYTES);
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let image = image.thumbnail(MAX_COVER_EDGE, MAX_COVER_EDGE);
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png).ok()?;
    Some(output.into_inner())
}

fn cache_directory() -> Option<PathBuf> {
    let cache_home = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    let directory = cache_home.join("qingyin/covers");
    std::fs::create_dir_all(&directory).ok()?;
    Some(directory)
}

fn database_path() -> Result<PathBuf, LibraryIoError> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or(LibraryIoError::MissingDataHome)?;
    let directory = data_home.join("qingyin");
    std::fs::create_dir_all(&directory).map_err(|source| LibraryIoError::CreateDataDir {
        path: directory.clone(),
        source,
    })?;
    Ok(directory.join("library.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn cached_cover_is_scaled_to_the_maximum_edge() {
        let image = image::DynamicImage::new_rgb8(1024, 256);
        let mut source = Cursor::new(Vec::new());
        image.write_to(&mut source, ImageFormat::Png).unwrap();

        let cached = prepare_cached_cover(source.get_ref()).unwrap();
        let dimensions = image::load_from_memory(&cached).unwrap();

        assert_eq!((dimensions.width(), dimensions.height()), (512, 128));
    }

    #[test]
    fn oversized_cover_source_is_rejected() {
        let source = vec![0; MAX_COVER_SOURCE_BYTES + 1];
        assert!(prepare_cached_cover(&source).is_none());
    }

    #[test]
    fn watch_status_includes_failed_counts() {
        let summary = WatchSummary {
            upserted: 2,
            removed: 1,
            failed: 0,
        };
        assert_eq!(watch_status(&summary), "已更新曲库：新增 2，删除 1");
        let failed = WatchSummary {
            upserted: 0,
            removed: 1,
            failed: 3,
        };
        assert_eq!(watch_status(&failed), "已更新曲库：新增 0，删除 1，失败 3");
    }

    #[test]
    fn reuses_cached_cover_urls_for_unchanged_paths() {
        let previous_tracks = snapshots_from(vec![test_track("A", "专辑", 10)]);
        let previous_covers = vec!["file:///cache/a.png".to_owned()];
        let tracks = snapshots_from(vec![
            test_track("A", "专辑", 10),
            test_track("B", "专辑", 12),
        ]);
        let covers = reuse_or_cache_covers(&previous_tracks, &previous_covers, &tracks);
        assert_eq!(covers[0], "file:///cache/a.png");
        assert_eq!(covers.len(), 2);
    }

    #[test]
    fn cover_cache_key_includes_path_and_mtime() {
        let path = Path::new("/music/a.flac");
        assert_eq!(cover_cache_stem(path, 1), cover_cache_stem(path, 1));
        assert_ne!(cover_cache_stem(path, 1), cover_cache_stem(path, 2));
        assert_ne!(
            cover_cache_stem(path, 1),
            cover_cache_stem(Path::new("/music/b.flac"), 1)
        );
    }

    #[test]
    fn aggregates_visible_library_into_artist_and_album_groups() {
        let tracks = snapshots_from(vec![
            TrackMetadata::from_display(
                "/music/a.flac",
                "A",
                Some("清音".into()),
                vec!["甲".into()],
                Some(Duration::from_secs(10)),
            ),
            TrackMetadata::from_display(
                "/music/b.flac",
                "B",
                Some("清音".into()),
                vec!["甲".into(), "乙".into()],
                Some(Duration::from_secs(12)),
            ),
        ]);
        let covers = vec!["cover-a".to_owned(), "cover-b".to_owned()];
        let artists = aggregate_artists(&tracks, &covers);
        let albums = aggregate_albums(&tracks, &covers);

        assert_eq!(artists.len(), 2);
        assert_eq!(artists[0].name, "甲");
        assert_eq!(artists[0].track_count, 2);
        assert_eq!(artists[1].name, "乙");
        assert_eq!(artists[1].track_count, 1);
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].name, "清音");
        assert_eq!(albums[0].track_count, 2);
        assert_eq!(albums[0].cover_url, "cover-a");
    }

    #[test]
    fn ignores_duplicate_music_directories() {
        let mut session = LibrarySession::default();
        assert!(session.add_music_directory(PathBuf::from("/music")));
        assert!(!session.add_music_directory(PathBuf::from("/music")));
        assert_eq!(session.music_directories(), &[PathBuf::from("/music")]);
    }

    #[test]
    fn scan_and_search_errors_are_matchable_with_chinese_messages() {
        let scan = LibraryIoError::OpenDatabase(StorageError::DurationOverflow);
        let search = LibraryIoError::SearchTracks(StorageError::DurationOverflow);
        assert!(matches!(scan, LibraryIoError::OpenDatabase(_)));
        assert!(matches!(search, LibraryIoError::SearchTracks(_)));
        assert_eq!(scan.to_user_message(), "无法打开曲库");
        assert_eq!(search.to_user_message(), "搜索失败");
        assert_eq!(
            LibraryIoError::MissingDataHome.to_user_message(),
            "无法确定用户数据目录"
        );
        assert_eq!(
            LibraryIoError::RestoreSpawn.to_user_message(),
            "无法启动曲库恢复"
        );
    }

    fn test_track(title: &str, album: &str, duration: u64) -> TrackMetadata {
        TrackMetadata::from_display(
            format!("/music/{title}.flac"),
            title,
            Some(album.to_owned()),
            Vec::new(),
            Some(Duration::from_secs(duration)),
        )
    }
}
