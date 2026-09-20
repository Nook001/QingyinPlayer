mod collections;
mod playback;
#[allow(dead_code)]
mod pointer_guard;

use collections::{CollectionModel, DetailTrackModel, TrackListModel};
use cstr::cstr;
use image::{ImageFormat, ImageReader, Limits};
use playback::PlaybackController;
use qingyin_chinese::compare_keys;
use qingyin_core::Settings;
use qingyin_library::{
    CollectionEntry, LibraryWatcher, MusicLibrary, ScanEvent, WatchSummary, aggregate_albums,
    aggregate_artists, is_library_track, prune_unwatched_tracks, watch_directories,
};
use qingyin_metadata::{CoverArt, TrackMetadata, read_cover};
use qingyin_storage::Database;
use qmetaobject::QUrl;
use qmetaobject::prelude::*;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Write;
use std::fs::OpenOptions;
use std::hash::{DefaultHasher, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tracing::warn;

const MAX_COVER_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_COVER_DECODE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_COVER_SOURCE_EDGE: u32 = 8192;
const MAX_COVER_EDGE: u32 = 512;
const SEARCH_RESULT_LIMIT: usize = 500;

#[derive(Debug)]
struct ScanResult {
    tracks: Vec<TrackMetadata>,
    cover_urls: Vec<String>,
    artists: Vec<CollectionEntry>,
    albums: Vec<CollectionEntry>,
    status: String,
}

#[derive(Debug)]
struct SearchResult {
    tracks: Vec<TrackMetadata>,
}

#[allow(missing_debug_implementations, clippy::struct_excessive_bools)]
#[derive(QObject, Default)]
pub struct AppBridge {
    base: qt_base_class!(trait QObject),
    /// Visible rows bound by the library `TrackTable` (search results or full library).
    library_model: qt_property!(RefCell<TrackListModel>; CONST),
    /// Full imported library, independent of the current search filter.
    library_tracks: Vec<TrackMetadata>,
    library_cover_urls: Vec<String>,
    playback: qt_property!(RefCell<PlaybackController>; CONST),
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
    dark_theme: qt_property!(bool; NOTIFY settings_changed),
    music_folders: qt_property!(QString; NOTIFY settings_changed),
    settings_changed: qt_signal!(),
    settings: Settings,
    restored: bool,
    watcher: Option<LibraryWatcher>,
    library_generation: u64,
    scanning: qt_property!(bool; NOTIFY scanning_changed),
    scanning_changed: qt_signal!(),
    scan_status: qt_property!(QString; NOTIFY scan_status_changed),
    scan_status_changed: qt_signal!(),
    application_name: qt_method!(
        fn application_name(&self) -> QString {
            let _ = self;
            "清音".into()
        }
    ),
    version: qt_method!(
        fn version(&self) -> QString {
            let _ = self;
            env!("CARGO_PKG_VERSION").into()
        }
    ),
    pointer_debug_enabled: qt_method!(
        fn pointer_debug_enabled(&self) -> bool {
            let _ = self;
            pointer_debug_from_env()
        }
    ),
    log_pointer: qt_method!(
        fn log_pointer(&self, kind: QString, target: QString, extra: QString) {
            let _ = self;
            let kind: String = kind.into();
            let target: String = target.into();
            let extra: String = extra.into();
            pointer_trace(&kind, &format!("{target}  {extra}"));
        }
    ),
    drop_pointer_grabs: qt_method!(
        fn drop_pointer_grabs(&self) {
            let _ = self;
        }
    ),
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
            let mut library_roots = self.settings.music_directories.clone();
            if !library_roots.iter().any(|existing| existing == &scan_path) {
                library_roots.push(scan_path.clone());
            }
            let bridge = QPointer::from(&*self);
            let apply_result =
                qmetaobject::queued_callback(move |result: Result<ScanResult, String>| {
                    let Some(bridge) = bridge.as_pinned() else {
                        return;
                    };
                    let mut bridge = bridge.borrow_mut();
                    bridge.scanning = false;
                    bridge.scanning_changed();
                    match result {
                        Ok(result) => {
                            if bridge.settings.add_music_directory(path.clone()) {
                                bridge.persist_settings();
                            }
                            bridge.apply_scan_result(result);
                            bridge.ensure_watcher();
                        }
                        Err(error) => {
                            bridge.scan_status = error.into();
                            bridge.scan_status_changed();
                        }
                    }
                });

            std::thread::spawn(move || {
                apply_result(scan_music_directory(&scan_path, &library_roots));
            });
        }
    ),
    play_track: qt_method!(
        fn play_track(&mut self, row: i32) {
            pointer_trace("slot", &format!("play_track row={row}"));
            let (tracks, covers) = self.library_model.borrow().snapshot();
            self.playback
                .borrow_mut()
                .play_from_list(tracks, covers, row);
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
    set_dark_theme: qt_method!(
        fn set_dark_theme(&mut self, dark: bool) {
            self.set_dark_theme_internal(dark);
        }
    ),
    restore_session: qt_method!(
        fn restore_session(&mut self) {
            self.restore_session_internal();
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
            pointer_trace("slot", &format!("play_artist_track row={row}"));
            let (tracks, covers) = self.artist_detail.borrow().snapshot();
            self.playback
                .borrow_mut()
                .play_from_list(tracks, covers, row);
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
            pointer_trace("slot", &format!("play_album_track row={row}"));
            let (tracks, covers) = self.album_detail.borrow().snapshot();
            self.playback
                .borrow_mut()
                .play_from_list(tracks, covers, row);
        }
    ),
    shutdown: qt_method!(
        fn shutdown(&mut self) {
            self.shutdown_internal();
        }
    ),
}

impl AppBridge {
    fn shutdown_internal(&mut self) {
        self.watcher = None;
        self.playback.borrow_mut().shutdown();
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

        let bridge = QPointer::from(&*self);
        let apply_result =
            qmetaobject::queued_callback(move |result: Result<SearchResult, String>| {
                let Some(bridge) = bridge.as_pinned() else {
                    return;
                };
                let mut bridge = bridge.borrow_mut();
                if bridge.search_generation != generation {
                    return;
                }
                bridge.searching = false;
                match result {
                    Ok(result) => {
                        let count = result.tracks.len();
                        let cover_urls = result
                            .tracks
                            .iter()
                            .map(|track| bridge.library_cover_for_path(&track.path))
                            .collect();
                        let apply_column_sort = !bridge.sort_column.is_empty();
                        bridge.replace_visible_tracks(result.tracks, cover_urls, apply_column_sort);
                        bridge.search_status = format!("找到 {count} 首歌曲").into();
                    }
                    Err(error) => bridge.search_status = error.into(),
                }
                bridge.search_changed();
            });

        let roots = self.settings.music_directories.clone();
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
        self.settings.sort_column = self.sort_column.to_string();
        self.settings.sort_ascending = self.sort_ascending;
        self.persist_settings();
        self.sort_visible_tracks();
        self.sort_changed();
    }

    fn set_dark_theme_internal(&mut self, dark: bool) {
        if self.dark_theme == dark {
            return;
        }
        self.dark_theme = dark;
        self.settings.dark_theme = dark;
        self.persist_settings();
        self.settings_changed();
    }

    fn restore_session_internal(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        self.attach_playback_persist();
        self.settings = Settings::load_or_default();
        self.apply_settings_to_ui();
        self.scan_status = "正在恢复曲库…".into();
        self.scan_status_changed();

        let bridge = QPointer::from(&*self);
        let apply_result =
            qmetaobject::queued_callback(move |result: Result<ScanResult, String>| {
                let Some(bridge) = bridge.as_pinned() else {
                    return;
                };
                let mut bridge = bridge.borrow_mut();
                match result {
                    Ok(result) => bridge.apply_scan_result(result),
                    Err(error) => {
                        bridge.scan_status = error.into();
                        bridge.scan_status_changed();
                    }
                }
                let directories = bridge.settings.music_directories.clone();
                if !directories.is_empty() {
                    bridge.scan_directories(directories);
                }
                bridge.ensure_watcher();
            });
        let directories = self.settings.music_directories.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("qingyin-restore".into())
            .spawn(move || apply_result(load_stored_library(&directories)))
        {
            warn!(%error, "failed to start library restore");
            self.scan_status = "无法启动曲库恢复".into();
            self.scan_status_changed();
        }
    }

    fn apply_settings_to_ui(&mut self) {
        self.dark_theme = self.settings.dark_theme;
        self.playback
            .borrow_mut()
            .apply_saved_volume(self.settings.volume);
        self.sort_column = self.settings.sort_column.clone().into();
        self.sort_ascending = self.settings.sort_ascending;
        self.music_folders = format_music_folders(&self.settings.music_directories).into();
        self.settings_changed();
        self.sort_changed();
    }

    fn persist_settings(&mut self) {
        self.settings.volume = self.playback.borrow().volume();
        self.music_folders = format_music_folders(&self.settings.music_directories).into();
        self.settings_changed();
        if let Err(error) = self.settings.save() {
            warn!(%error, "failed to save settings");
        }
    }

    fn attach_playback_persist(&mut self) {
        let bridge = QPointer::from(&*self);
        let persist = qmetaobject::queued_callback(move |volume: f64| {
            let Some(bridge) = bridge.as_pinned() else {
                return;
            };
            let mut bridge = bridge.borrow_mut();
            if (bridge.settings.volume - volume).abs() > f64::EPSILON {
                bridge.settings.volume = volume;
                bridge.persist_settings();
            }
        });
        self.playback.borrow_mut().set_volume_persist(persist);
    }

    fn ensure_watcher(&mut self) {
        self.watcher = None;
        let directories = self.settings.music_directories.clone();
        if directories.is_empty() {
            return;
        }
        let Ok(database_path) = database_path() else {
            warn!("unable to determine library database path for watcher");
            return;
        };
        let bridge = QPointer::from(&*self);
        let apply_batch = qmetaobject::queued_callback(move |summary: WatchSummary| {
            let Some(bridge) = bridge.as_pinned() else {
                return;
            };
            bridge.borrow_mut().apply_watch_summary(&summary);
        });
        match watch_directories(directories, database_path, apply_batch) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(error) => warn!(%error, "failed to watch music directories"),
        }
    }

    fn apply_watch_summary(&mut self, summary: &WatchSummary) {
        if !summary.has_library_changes() {
            if summary.failed > 0 {
                self.scan_status = format!("曲库更新失败 {} 项", summary.failed).into();
                self.scan_status_changed();
            }
            return;
        }

        match load_stored_tracks() {
            Ok(tracks) => {
                let tracks = tracks_in_library(tracks, &self.settings.music_directories);
                let cover_urls =
                    reuse_or_cache_covers(&self.library_tracks, &self.library_cover_urls, &tracks);
                let status = watch_status(summary);
                self.library_generation = self.library_generation.wrapping_add(1);
                let generation = self.library_generation;
                let bridge = QPointer::from(&*self);
                let apply_result = qmetaobject::queued_callback(move |result: ScanResult| {
                    let Some(bridge) = bridge.as_pinned() else {
                        return;
                    };
                    let mut bridge = bridge.borrow_mut();
                    if bridge.library_generation != generation {
                        return;
                    }
                    bridge.apply_scan_result(result);
                    if !bridge.search_query.is_empty() {
                        let query = bridge.search_query.clone();
                        bridge.search_tracks_internal(&query);
                    }
                    bridge.playback.borrow_mut().skip_missing_current_track();
                });
                std::thread::spawn(move || {
                    apply_result(scan_result_with_collections(tracks, cover_urls, status));
                });
            }
            Err(error) => {
                self.scan_status = error.into();
                self.scan_status_changed();
            }
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

        let bridge = QPointer::from(&*self);
        let apply_result =
            qmetaobject::queued_callback(move |result: Result<ScanResult, String>| {
                let Some(bridge) = bridge.as_pinned() else {
                    return;
                };
                let mut bridge = bridge.borrow_mut();
                bridge.scanning = false;
                bridge.scanning_changed();
                match result {
                    Ok(result) => bridge.apply_scan_result(result),
                    Err(error) => {
                        bridge.scan_status = error.into();
                        bridge.scan_status_changed();
                    }
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
        tracks: Vec<TrackMetadata>,
        cover_urls: Vec<String>,
        apply_column_sort: bool,
    ) {
        let current_path = self.playback.borrow().current_path();
        let mut tracks = tracks;
        let mut cover_urls = cover_urls;
        if apply_column_sort {
            sort_tracks(
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
        self.playback
            .borrow_mut()
            .set_current_index_if_idle(position);
    }

    fn sort_visible_tracks(&mut self) {
        self.library_model
            .borrow_mut()
            .sort_in_place(&self.sort_column.to_string(), self.sort_ascending);
    }

    fn library_cover_for_path(&self, path: &Path) -> String {
        self.library_tracks
            .iter()
            .position(|track| track.path == path)
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

pub fn register_qml_types() {
    qml_register_type::<AppBridge>(cstr!("Qingyin"), 1, 0, cstr!("AppBridge"));
    qml_register_type::<TrackListModel>(cstr!("Qingyin"), 1, 0, cstr!("TrackListModel"));
    qml_register_type::<PlaybackController>(cstr!("Qingyin"), 1, 0, cstr!("PlaybackController"));
}

fn pointer_log_path() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| std::env::temp_dir().join("qingyin-pointer.log"))
}

fn pointer_debug_from_env() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let enabled = match std::env::var("QINGYIN_POINTER_DEBUG") {
            Ok(value) => matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
            Err(_) => false,
        };
        if enabled {
            eprintln!("[qingyin-pointer] writing {}", pointer_log_path().display());
        }
        enabled
    })
}

pub(crate) fn pointer_trace(kind: &str, detail: &str) {
    if !pointer_debug_from_env() {
        return;
    }
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis();
    let line = format!("[qingyin-pointer] {millis}  {kind}  {detail}");
    eprintln!("{line}");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(pointer_log_path())
    {
        let _ = std::io::Write::write_all(&mut file, format!("{line}\n").as_bytes());
    }
}

fn scan_music_directory(path: &Path, library_roots: &[PathBuf]) -> Result<ScanResult, String> {
    scan_music_directories(&[path.to_path_buf()], library_roots)
}

fn scan_music_directories(
    directories: &[PathBuf],
    library_roots: &[PathBuf],
) -> Result<ScanResult, String> {
    let mut database = Database::open(database_path()?).map_err(|error| error.to_string())?;
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
    prune_unwatched_tracks(&database, library_roots).map_err(|error| error.to_string())?;
    let tracks = listed_tracks(&database)?;
    let cover_urls = existing_cover_urls(&tracks, false);
    Ok(scan_result_with_collections(
        tracks,
        cover_urls,
        format!("扫描完成：导入 {imported} 首，跳过 {unchanged} 首，失败 {failed} 首"),
    ))
}

fn load_stored_library(roots: &[PathBuf]) -> Result<ScanResult, String> {
    let database = Database::open(database_path()?).map_err(|error| error.to_string())?;
    prune_unwatched_tracks(&database, roots).map_err(|error| error.to_string())?;
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
    tracks: Vec<TrackMetadata>,
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

fn load_stored_tracks() -> Result<Vec<TrackMetadata>, String> {
    listed_tracks(&Database::open(database_path()?).map_err(|error| error.to_string())?)
}

fn listed_tracks(database: &Database) -> Result<Vec<TrackMetadata>, String> {
    database
        .list_tracks()
        .map_err(|error| error.to_string())
        .map(|tracks| {
            tracks
                .into_iter()
                .map(|track| track.metadata)
                .collect::<Vec<_>>()
        })
}

fn tracks_in_library(tracks: Vec<TrackMetadata>, roots: &[PathBuf]) -> Vec<TrackMetadata> {
    tracks
        .into_iter()
        .filter(|track| is_library_track(&track.path, roots))
        .collect()
}

fn normalize_music_directory(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn reuse_or_cache_covers(
    previous_tracks: &[TrackMetadata],
    previous_covers: &[String],
    tracks: &[TrackMetadata],
) -> Vec<String> {
    let mut previous = HashMap::new();
    for (track, cover) in previous_tracks.iter().zip(previous_covers) {
        if !cover.is_empty() {
            previous.insert(track.path.clone(), (track.modified_at, cover.clone()));
        }
    }
    let directory = cache_directory();
    tracks
        .iter()
        .map(|track| {
            if let Some((modified_at, cover)) = previous.get(&track.path)
                && *modified_at == track.modified_at
            {
                return cover.clone();
            }
            directory
                .as_ref()
                .and_then(|directory| resolve_cover_url(directory, track, true))
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

fn format_music_folders(directories: &[PathBuf]) -> String {
    directories
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn search_database(query: &str, roots: &[PathBuf]) -> Result<SearchResult, String> {
    let database = Database::open(database_path()?).map_err(|error| error.to_string())?;
    let tracks = database
        .search_tracks(query, SEARCH_RESULT_LIMIT)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|track| track.metadata)
        .filter(|track| is_library_track(&track.path, roots))
        .collect();
    Ok(SearchResult { tracks })
}

pub(crate) fn sort_tracks(
    tracks: &mut Vec<TrackMetadata>,
    cover_urls: &mut Vec<String>,
    column: &str,
    ascending: bool,
) {
    let ascending = column.is_empty() || ascending;
    cover_urls.resize(tracks.len(), String::new());
    let mut rows = std::mem::take(tracks)
        .into_iter()
        .zip(std::mem::take(cover_urls))
        .collect::<Vec<_>>();
    rows.sort_by(|(left, _), (right, _)| {
        let ordering = compare_tracks(left, right, column).then_with(|| left.path.cmp(&right.path));
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
    let (sorted_tracks, sorted_covers) = rows.into_iter().unzip();
    *tracks = sorted_tracks;
    *cover_urls = sorted_covers;
}

fn compare_tracks(left: &TrackMetadata, right: &TrackMetadata, column: &str) -> Ordering {
    match column {
        "album" => compare_keys(&left.album_key, &right.album_key),
        "duration" => left.duration.cmp(&right.duration),
        _ => compare_keys(&left.title_key, &right.title_key),
    }
}

fn existing_cover_urls(tracks: &[TrackMetadata], read_source: bool) -> Vec<String> {
    let Some(directory) = cache_directory() else {
        return vec![String::new(); tracks.len()];
    };
    tracks
        .iter()
        .map(|track| resolve_cover_url(&directory, track, read_source).unwrap_or_default())
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

pub(crate) const fn previous_track_index(current: usize) -> usize {
    current.saturating_sub(1)
}

pub(crate) const fn next_track_index(current: usize, track_count: usize) -> Option<usize> {
    match current.checked_add(1) {
        Some(next) if next < track_count => Some(next),
        _ => None,
    }
}

fn cache_directory() -> Option<PathBuf> {
    let cache_home = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    let directory = cache_home.join("qingyin/covers");
    std::fs::create_dir_all(&directory).ok()?;
    Some(directory)
}

fn database_path() -> Result<PathBuf, String> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| "无法确定用户数据目录".to_owned())?;
    let directory = data_home.join("qingyin");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("library.sqlite3"))
}

pub(crate) fn format_duration(track: &TrackMetadata) -> String {
    let seconds = track.duration.map_or(0, |duration| duration.as_secs());
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub(crate) fn duration_millis(duration: Option<Duration>) -> i64 {
    duration.map_or(0, |duration| {
        i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_navigation_stops_at_list_boundaries() {
        assert_eq!(previous_track_index(0), 0);
        assert_eq!(previous_track_index(3), 2);
        assert_eq!(next_track_index(0, 2), Some(1));
        assert_eq!(next_track_index(1, 2), None);
        assert_eq!(next_track_index(usize::MAX, usize::MAX), None);
    }

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
    fn sorts_tracks_by_selected_column_and_direction() {
        let mut tracks = vec![test_track("B", "专辑甲", 20), test_track("A", "专辑乙", 10)];
        let mut covers = vec!["B封面".to_owned(), "A封面".to_owned()];

        sort_tracks(&mut tracks, &mut covers, "title", true);
        assert_eq!(tracks[0].title, "A");
        assert_eq!(tracks[1].title, "B");
        assert_eq!(covers, ["A封面", "B封面"]);

        sort_tracks(&mut tracks, &mut covers, "duration", false);
        assert_eq!(tracks[0].duration, Some(Duration::from_secs(20)));
        assert_eq!(tracks[1].duration, Some(Duration::from_secs(10)));
        assert_eq!(covers, ["B封面", "A封面"]);
    }

    #[test]
    fn sorts_han_titles_among_latin_titles() {
        let mut tracks = vec![
            test_track("周杰伦", "专辑", 10),
            test_track("Adele", "专辑", 10),
            test_track("阿妹", "专辑", 10),
        ];
        let mut covers = vec!["z".into(), "a".into(), "m".into()];
        sort_tracks(&mut tracks, &mut covers, "title", true);
        assert_eq!(
            tracks
                .iter()
                .map(|track| track.title.as_str())
                .collect::<Vec<_>>(),
            ["Adele", "阿妹", "周杰伦"]
        );
    }

    #[test]
    fn formats_music_folders_on_separate_lines() {
        let folders = format_music_folders(&[PathBuf::from("/music/a"), PathBuf::from("/music/b")]);
        assert_eq!(folders, "/music/a\n/music/b");
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
        let previous_tracks = vec![test_track("A", "专辑", 10)];
        let previous_covers = vec!["file:///cache/a.png".to_owned()];
        let tracks = vec![test_track("A", "专辑", 10), test_track("B", "专辑", 12)];
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
        let tracks = vec![
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
        ];
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
