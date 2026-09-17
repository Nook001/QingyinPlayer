use cstr::cstr;
use qingyin_library::MusicLibrary;
use qingyin_metadata::TrackMetadata;
use qingyin_player::{PlaybackState, Player};
use qingyin_storage::Database;
use qmetaobject::QUrl;
use qmetaobject::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const TITLE_ROLE: i32 = 0x0100;
const ARTIST_ROLE: i32 = TITLE_ROLE + 1;
const ALBUM_ROLE: i32 = TITLE_ROLE + 2;
const DURATION_ROLE: i32 = TITLE_ROLE + 3;
const PATH_ROLE: i32 = TITLE_ROLE + 4;

#[derive(Debug)]
struct ScanResult {
    tracks: Vec<TrackMetadata>,
    status: String,
}

#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct AppBridge {
    base: qt_base_class!(trait QAbstractListModel),
    tracks: Vec<TrackMetadata>,
    scanning: qt_property!(bool; NOTIFY scanning_changed),
    scanning_changed: qt_signal!(),
    scan_status: qt_property!(QString; NOTIFY scan_status_changed),
    scan_status_changed: qt_signal!(),
    player: Option<Player>,
    playback_state: qt_property!(QString; NOTIFY playback_changed),
    current_title: qt_property!(QString; NOTIFY playback_changed),
    current_artist: qt_property!(QString; NOTIFY playback_changed),
    playback_error: qt_property!(QString; NOTIFY playback_changed),
    playback_changed: qt_signal!(),
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
            let Some(track) = usize::try_from(row)
                .ok()
                .and_then(|row| self.tracks.get(row))
                .cloned()
            else {
                return;
            };
            if self.player.is_none() {
                match Player::initialize() {
                    Ok(player) => self.player = Some(player),
                    Err(error) => {
                        self.playback_error = error.to_string().into();
                        self.playback_changed();
                        return;
                    }
                }
            }
            let player = self.player.as_mut().expect("player was initialized");

            if let Err(error) = player.load(&track.path).and_then(|()| player.play()) {
                self.playback_state = "stopped".into();
                self.playback_error = error.to_string().into();
                self.playback_changed();
                return;
            }
            self.current_title = track.title.into();
            self.current_artist = track.artists.join("、").into();
            self.playback_state = "playing".into();
            self.playback_error = QString::default();
            self.playback_changed();
        }
    ),
    toggle_playback: qt_method!(
        fn toggle_playback(&mut self) {
            let Some(player) = self.player.as_mut() else {
                return;
            };
            let result = match player.state() {
                PlaybackState::Playing => player.pause(),
                PlaybackState::Paused | PlaybackState::Stopped => player.play(),
            };
            if let Err(error) = result {
                self.playback_error = error.to_string().into();
                self.playback_changed();
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
    ),
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

    Ok(ScanResult {
        tracks: library.tracks().to_vec(),
        status,
    })
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
