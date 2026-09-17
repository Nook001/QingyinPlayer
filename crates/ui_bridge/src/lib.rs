use cstr::cstr;
use image::{ImageFormat, ImageReader, Limits};
use qingyin_library::MusicLibrary;
use qingyin_metadata::{TrackMetadata, read_cover};
use qingyin_player::{PlaybackState, Player, PlayerEvent};
use qingyin_storage::Database;
use qmetaobject::QUrl;
use qmetaobject::prelude::*;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TITLE_ROLE: i32 = 0x0100;
const ARTIST_ROLE: i32 = TITLE_ROLE + 1;
const ALBUM_ROLE: i32 = TITLE_ROLE + 2;
const DURATION_ROLE: i32 = TITLE_ROLE + 3;
const PATH_ROLE: i32 = TITLE_ROLE + 4;
const COVER_ROLE: i32 = TITLE_ROLE + 5;
const MAX_COVER_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_COVER_DECODE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_COVER_SOURCE_EDGE: u32 = 8192;
const MAX_COVER_EDGE: u32 = 512;

#[derive(Debug)]
struct ScanResult {
    tracks: Vec<TrackMetadata>,
    cover_urls: Vec<String>,
    status: String,
}

#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct AppBridge {
    base: qt_base_class!(trait QAbstractListModel),
    tracks: Vec<TrackMetadata>,
    cover_urls: Vec<String>,
    scanning: qt_property!(bool; NOTIFY scanning_changed),
    scanning_changed: qt_signal!(),
    scan_status: qt_property!(QString; NOTIFY scan_status_changed),
    scan_status_changed: qt_signal!(),
    player: Option<Player>,
    current_index: Option<usize>,
    playback_state: qt_property!(QString; NOTIFY playback_changed),
    current_title: qt_property!(QString; NOTIFY playback_changed),
    current_artist: qt_property!(QString; NOTIFY playback_changed),
    current_cover: qt_property!(QString; NOTIFY playback_changed),
    playback_error: qt_property!(QString; NOTIFY playback_changed),
    playback_changed: qt_signal!(),
    playback_position: qt_property!(i64; NOTIFY playback_progress_changed),
    playback_duration: qt_property!(i64; NOTIFY playback_progress_changed),
    player_volume: qt_property!(f64; NOTIFY playback_progress_changed),
    playback_progress_changed: qt_signal!(),
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
            self.scanning = true;
            self.scanning_changed();
            self.scan_status = "正在扫描音乐文件…".into();
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
                        Ok(result) => {
                            bridge.begin_reset_model();
                            bridge.tracks = result.tracks;
                            bridge.cover_urls = result.cover_urls;
                            bridge.end_reset_model();
                            bridge.scan_status = result.status.into();
                        }
                        Err(error) => bridge.scan_status = error.into(),
                    }
                    bridge.scan_status_changed();
                });

            std::thread::spawn(move || apply_result(scan_music_directory(&path)));
        }
    ),
    play_track: qt_method!(
        fn play_track(&mut self, row: i32) {
            self.play_row(row);
        }
    ),
    toggle_playback: qt_method!(
        fn toggle_playback(&mut self) {
            self.toggle_playback_internal();
        }
    ),
    play_previous: qt_method!(
        fn play_previous(&mut self) {
            let Some(current) = self.current_index else {
                return;
            };
            self.play_row(i32::try_from(previous_track_index(current)).unwrap_or(0));
        }
    ),
    play_next: qt_method!(
        fn play_next(&mut self) {
            self.play_next_internal();
        }
    ),
    refresh_playback_progress: qt_method!(
        fn refresh_playback_progress(&mut self) {
            self.refresh_playback_progress_internal();
        }
    ),
    seek_to: qt_method!(
        fn seek_to(&mut self, position: i64) {
            self.seek_to_internal(position);
        }
    ),
    set_player_volume: qt_method!(
        fn set_player_volume(&mut self, volume: f64) {
            self.set_player_volume_internal(volume);
        }
    ),
}

impl AppBridge {
    fn ensure_player(&mut self) -> Result<(), String> {
        if self.player.is_some() {
            return Ok(());
        }

        let mut player = Player::initialize().map_err(|error| error.to_string())?;
        let bridge = QPointer::from(&*self);
        let dispatch = qmetaobject::queued_callback(move |event: PlayerEvent| {
            let Some(bridge) = bridge.as_pinned() else {
                return;
            };
            bridge.borrow_mut().handle_player_event(event);
        });
        player
            .set_event_handler(dispatch)
            .map_err(|error| error.to_string())?;
        self.player_volume = player.volume();
        self.player = Some(player);
        self.playback_progress_changed();
        Ok(())
    }

    fn play_row(&mut self, row: i32) {
        let Some((row, track)) = usize::try_from(row)
            .ok()
            .and_then(|row| self.tracks.get(row).cloned().map(|track| (row, track)))
        else {
            return;
        };
        if let Err(error) = self.ensure_player() {
            self.set_playback_error(error);
            return;
        }
        let player = self.player.as_mut().expect("player was initialized");
        if let Err(error) = player.load(&track.path).and_then(|()| player.play()) {
            self.set_playback_error(error.to_string());
            return;
        }

        self.current_index = Some(row);
        self.current_title = track.title.into();
        self.current_artist = track.artists.join("、").into();
        self.current_cover = self.cover_urls.get(row).cloned().unwrap_or_default().into();
        self.playback_state = "playing".into();
        self.playback_error = QString::default();
        self.playback_position = 0;
        self.playback_duration = duration_millis(track.duration);
        self.playback_changed();
        self.playback_progress_changed();
    }

    fn toggle_playback_internal(&mut self) {
        let Some(state) = self.player.as_ref().map(Player::state) else {
            return;
        };
        if state == PlaybackState::Stopped {
            if let Some(row) = self.current_index.and_then(|row| i32::try_from(row).ok()) {
                self.play_row(row);
            }
            return;
        }

        let player = self.player.as_mut().expect("player exists");
        let result = match state {
            PlaybackState::Playing => player.pause(),
            PlaybackState::Paused => player.play(),
            PlaybackState::Stopped => unreachable!(),
        };
        if let Err(error) = result {
            self.set_playback_error(error.to_string());
            return;
        }
        self.playback_state = match player.state() {
            PlaybackState::Playing => "playing".into(),
            PlaybackState::Paused => "paused".into(),
            PlaybackState::Stopped => "stopped".into(),
        };
        self.playback_error = QString::default();
        self.playback_changed();
    }

    fn play_next_internal(&mut self) {
        let Some(next) = self
            .current_index
            .and_then(|current| next_track_index(current, self.tracks.len()))
        else {
            if self.current_index.is_some() {
                if let Some(player) = self.player.as_mut() {
                    let _ = player.stop();
                }
                self.playback_state = "stopped".into();
                self.playback_position = 0;
                self.playback_changed();
                self.playback_progress_changed();
            }
            return;
        };
        if let Ok(next) = i32::try_from(next) {
            self.play_row(next);
        }
    }

    fn refresh_playback_progress_internal(&mut self) {
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let position = duration_millis(player.position());
        let duration = duration_millis(player.duration());
        if position != self.playback_position
            || (duration > 0 && duration != self.playback_duration)
        {
            self.playback_position = position;
            if duration > 0 {
                self.playback_duration = duration;
            }
            self.playback_progress_changed();
        }
    }

    fn seek_to_internal(&mut self, position: i64) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        let maximum = self.playback_duration.max(0);
        let position = position.clamp(0, maximum);
        let Ok(position_millis) = u64::try_from(position) else {
            return;
        };
        if let Err(error) = player.seek(Duration::from_millis(position_millis)) {
            self.set_playback_error(error.to_string());
            return;
        }
        self.playback_position = position;
        self.playback_progress_changed();
    }

    fn set_player_volume_internal(&mut self, volume: f64) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        player.set_volume(volume);
        self.player_volume = player.volume();
        self.playback_progress_changed();
    }

    fn handle_player_event(&mut self, event: PlayerEvent) {
        match event {
            PlayerEvent::EndOfStream => self.play_next_internal(),
            PlayerEvent::Error(error) => self.set_playback_error(error),
        }
    }

    fn set_playback_error(&mut self, error: String) {
        self.playback_state = "stopped".into();
        self.playback_error = error.into();
        self.playback_changed();
    }
}

impl QAbstractListModel for AppBridge {
    fn row_count(&self) -> i32 {
        i32::try_from(self.tracks.len()).unwrap_or(i32::MAX)
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let Some(track) = usize::try_from(index.row())
            .ok()
            .and_then(|row| self.tracks.get(row))
        else {
            return QVariant::default();
        };

        match role {
            TITLE_ROLE => QString::from(track.title.clone()).into(),
            ARTIST_ROLE => QString::from(track.artists.join("、")).into(),
            ALBUM_ROLE => QString::from(track.album.clone().unwrap_or_default()).into(),
            DURATION_ROLE => QString::from(format_duration(track)).into(),
            PATH_ROLE => QString::from(track.path.to_string_lossy().into_owned()).into(),
            COVER_ROLE => QString::from(
                self.cover_urls
                    .get(usize::try_from(index.row()).unwrap_or(usize::MAX))
                    .cloned()
                    .unwrap_or_default(),
            )
            .into(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        HashMap::from([
            (TITLE_ROLE, "title".into()),
            (ARTIST_ROLE, "artist".into()),
            (ALBUM_ROLE, "album".into()),
            (DURATION_ROLE, "duration".into()),
            (PATH_ROLE, "path".into()),
            (COVER_ROLE, "cover".into()),
        ])
    }
}

pub fn register_qml_types() {
    qml_register_type::<AppBridge>(cstr!("Qingyin"), 1, 0, cstr!("AppBridge"));
}

fn scan_music_directory(path: &Path) -> Result<ScanResult, String> {
    let database_path = database_path()?;
    let mut database = Database::open(database_path).map_err(|error| error.to_string())?;
    let mut library = MusicLibrary::default();
    let summary = library
        .scan_directory(&mut database, path)
        .map_err(|error| error.to_string())?;
    let failed = summary.failed.len();
    let status = format!(
        "扫描完成：导入 {} 首，跳过 {} 首，失败 {} 首",
        summary.imported, summary.unchanged, failed
    );
    let cover_urls = cache_cover_urls(library.tracks());

    Ok(ScanResult {
        tracks: library.tracks().to_vec(),
        cover_urls,
        status,
    })
}

fn cache_cover_urls(tracks: &[TrackMetadata]) -> Vec<String> {
    let Some(directory) = cache_directory() else {
        return vec![String::new(); tracks.len()];
    };

    tracks
        .iter()
        .map(|track| cache_cover(&directory, &track.path).unwrap_or_default())
        .collect()
}

fn cache_cover(directory: &Path, track_path: &Path) -> Option<String> {
    let cover = read_cover(track_path).ok()??;
    let mut hasher = DefaultHasher::new();
    hasher.write(&cover.data);
    let path = directory.join(format!("{:016x}.png", hasher.finish()));
    if !path.exists() {
        let cached_cover = prepare_cached_cover(&cover.data)?;
        if std::fs::write(&path, cached_cover).is_err() {
            return None;
        }
    }
    url::Url::from_file_path(path).ok().map(Into::into)
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

const fn previous_track_index(current: usize) -> usize {
    current.saturating_sub(1)
}

const fn next_track_index(current: usize, track_count: usize) -> Option<usize> {
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

fn format_duration(track: &TrackMetadata) -> String {
    let seconds = track.duration.map_or(0, |duration| duration.as_secs());
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn duration_millis(duration: Option<Duration>) -> i64 {
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
}
