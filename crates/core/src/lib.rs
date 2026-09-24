mod paths;

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use paths::{PathError, XdgDirs};

const SETTINGS_VERSION: u32 = 1;
const DEFAULT_VOLUME: f64 = 1.0;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse settings: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize settings: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error(transparent)]
    Paths(#[from] PathError),
    #[error("settings version {found} is newer than supported version {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
}

/// Distinguishes a successful read from a missing file and an unreadable file.
#[derive(Debug)]
pub enum SettingsLoad {
    Loaded(Settings),
    Missing(Settings),
    Failed {
        error: SettingsError,
        fallback: Settings,
    },
}

impl SettingsLoad {
    #[must_use]
    pub fn settings(&self) -> &Settings {
        match self {
            Self::Loaded(settings) | Self::Missing(settings) => settings,
            Self::Failed { fallback, .. } => fallback,
        }
    }

    #[must_use]
    pub const fn can_prune_library(&self) -> bool {
        matches!(self, Self::Loaded(_))
    }

    #[must_use]
    pub fn into_settings(self) -> Settings {
        match self {
            Self::Loaded(settings) | Self::Missing(settings) => settings,
            Self::Failed { fallback, .. } => fallback,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortColumn {
    #[default]
    Title,
    Album,
    Duration,
}

impl SortColumn {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "" | "title" => Some(Self::Title),
            "album" => Some(Self::Album),
            "duration" => Some(Self::Duration),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Album => "album",
            Self::Duration => "duration",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

impl SortOrder {
    #[must_use]
    pub const fn from_ascending(ascending: bool) -> Self {
        if ascending {
            Self::Ascending
        } else {
            Self::Descending
        }
    }

    #[must_use]
    pub const fn is_ascending(self) -> bool {
        matches!(self, Self::Ascending)
    }

    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayMode {
    #[default]
    Sequential,
    Shuffle,
    RepeatOne,
}

impl PlayMode {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "" | "sequential" => Some(Self::Sequential),
            "shuffle" => Some(Self::Shuffle),
            "repeatOne" | "repeat_one" | "repeatone" => Some(Self::RepeatOne),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sequential => "sequential",
            Self::Shuffle => "shuffle",
            Self::RepeatOne => "repeatOne",
        }
    }

    #[must_use]
    pub const fn cycled(self) -> Self {
        match self {
            Self::Sequential => Self::Shuffle,
            Self::Shuffle => Self::RepeatOne,
            Self::RepeatOne => Self::Sequential,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub music_directories: Vec<PathBuf>,
    /// Legacy flag. Empty `color_theme` migrates from this value, then it is no longer written.
    #[serde(default, skip_serializing)]
    pub dark_theme: bool,
    #[serde(default)]
    pub color_theme: String,
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(default, deserialize_with = "deserialize_sort_column")]
    pub sort_column: SortColumn,
    #[serde(default = "default_sort_ascending", alias = "sort_ascending")]
    pub sort_ascending: bool,
    #[serde(default, deserialize_with = "deserialize_play_mode")]
    pub play_mode: PlayMode,
    #[serde(default)]
    pub playback_track_id: i64,
    #[serde(default)]
    pub playback_playlist_id: i64,
    #[serde(default)]
    pub playback_position_ms: i64,
    #[serde(default)]
    pub playback_queue_ids: Vec<i64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            music_directories: Vec::new(),
            dark_theme: false,
            color_theme: "qingci".into(),
            volume: DEFAULT_VOLUME,
            sort_column: SortColumn::Title,
            sort_ascending: true,
            play_mode: PlayMode::Sequential,
            playback_track_id: 0,
            playback_playlist_id: 0,
            playback_position_ms: 0,
            playback_queue_ids: Vec::new(),
        }
    }
}

impl Settings {
    /// Loads settings, preserving the difference between missing and unreadable files.
    #[must_use]
    pub fn load_status() -> SettingsLoad {
        match settings_path() {
            Ok(path) => Self::load_status_from(path),
            Err(error) => SettingsLoad::Failed {
                error,
                fallback: Self::default(),
            },
        }
    }

    /// Loads settings from `path` without collapsing errors into empty roots.
    #[must_use]
    pub fn load_status_from(path: impl AsRef<Path>) -> SettingsLoad {
        let path = path.as_ref();
        match fs::metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                SettingsLoad::Missing(Self::default())
            }
            Err(error) => SettingsLoad::Failed {
                error: SettingsError::Io {
                    path: path.to_path_buf(),
                    source: error,
                },
                fallback: Self::default(),
            },
            Ok(_) => match Self::load_from(path) {
                Ok(settings) => SettingsLoad::Loaded(settings),
                Err(error) => SettingsLoad::Failed {
                    error,
                    fallback: Self::default(),
                },
            },
        }
    }

    /// Loads settings from the default config path.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError`] when the file exists but cannot be read, parsed, or is too new.
    pub fn load() -> Result<Self, SettingsError> {
        Self::load_from(settings_path()?)
    }

    /// Loads settings from `path`. A missing file yields defaults.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError`] when the file exists but cannot be read, parsed, or is too new.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self, SettingsError> {
        let path = path.as_ref();
        match fs::read_to_string(path) {
            Ok(text) => {
                let mut settings = toml::from_str::<Self>(&text)?;
                settings.migrate()?;
                Ok(settings)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(SettingsError::Io {
                path: path.to_path_buf(),
                source: error,
            }),
        }
    }

    /// Writes settings to the default config path.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError`] when the config directory or file cannot be written.
    pub fn save(&self) -> Result<(), SettingsError> {
        self.save_to(settings_path()?)
    }

    /// Writes settings to `path` via a temporary file in the same directory, then replaces it.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError`] when the file cannot be serialized or written. A failed write
    /// leaves the previous file in place.
    pub fn save_to(&self, path: impl AsRef<Path>) -> Result<(), SettingsError> {
        let path = path.as_ref();
        if self.version > SETTINGS_VERSION {
            return Err(SettingsError::UnsupportedVersion {
                found: self.version,
                supported: SETTINGS_VERSION,
            });
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| SettingsError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let mut settings = self.clone();
        settings.prepare_for_save();
        let encoded = toml::to_string_pretty(&settings)?;
        atomic_write(path, encoded.as_bytes())
    }

    pub fn add_music_directory(&mut self, path: PathBuf) -> bool {
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

    #[must_use]
    pub const fn sort_order(&self) -> SortOrder {
        SortOrder::from_ascending(self.sort_ascending)
    }

    pub fn set_sort_order(&mut self, order: SortOrder) {
        self.sort_ascending = order.is_ascending();
    }

    fn migrate(&mut self) -> Result<(), SettingsError> {
        if self.version > SETTINGS_VERSION {
            return Err(SettingsError::UnsupportedVersion {
                found: self.version,
                supported: SETTINGS_VERSION,
            });
        }
        if self.version == 0 {
            self.version = SETTINGS_VERSION;
        }
        self.sanitize_values();
        Ok(())
    }

    fn prepare_for_save(&mut self) {
        self.version = SETTINGS_VERSION;
        self.sanitize_values();
    }

    fn sanitize_values(&mut self) {
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            DEFAULT_VOLUME
        };
        self.music_directories
            .retain(|path| !path.as_os_str().is_empty());
        if self.color_theme.is_empty() {
            self.color_theme = if self.dark_theme { "songyan" } else { "qingci" }.into();
        }
        self.color_theme = canonical_color_theme(&self.color_theme).to_string();
    }
}

/// Known color themes. An unknown name falls back to 青瓷.
#[must_use]
pub fn canonical_color_theme(name: &str) -> &'static str {
    match name {
        "jilan" => "jilan",
        "songyan" => "songyan",
        "mushan" => "mushan",
        "qingci" => "qingci",
        _ => "qingci",
    }
}

fn deserialize_sort_column<'de, D>(deserializer: D) -> Result<SortColumn, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(SortColumn::from_name(&value).unwrap_or_default())
}

fn deserialize_play_mode<'de, D>(deserializer: D) -> Result<PlayMode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(PlayMode::from_name(&value).unwrap_or_default())
}

fn default_version() -> u32 {
    SETTINGS_VERSION
}

fn default_volume() -> f64 {
    DEFAULT_VOLUME
}

const fn default_sort_ascending() -> bool {
    true
}

fn settings_path() -> Result<PathBuf, SettingsError> {
    Ok(XdgDirs::resolve()?.settings_file())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), SettingsError> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map_or_else(|| "settings.toml".into(), |name| name.to_os_string());
    let temp_path = directory.join(format!(
        ".{}.tmp-{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .map_err(|source| SettingsError::Io {
                path: temp_path.clone(),
                source,
            })?;
        file.write_all(bytes).map_err(|source| SettingsError::Io {
            path: temp_path.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| SettingsError::Io {
            path: temp_path.clone(),
            source,
        })?;
        drop(file);
        fs::rename(&temp_path, path).map_err(|source| SettingsError::Io {
            path: path.to_path_buf(),
            source,
        })
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_file_uses_defaults() {
        let path = unique_temp_path("missing.toml");
        let settings = Settings::load_from(&path).unwrap();
        assert_eq!(settings, Settings::default());
        assert!(matches!(
            Settings::load_status_from(&path),
            SettingsLoad::Missing(_)
        ));
        assert!(!Settings::load_status_from(&path).can_prune_library());
    }

    #[test]
    fn roundtrips_settings_and_clamps_volume() {
        let path = unique_temp_path("settings.toml");
        let mut settings = Settings {
            color_theme: "mushan".into(),
            volume: 1.8,
            sort_column: SortColumn::Album,
            sort_ascending: false,
            play_mode: PlayMode::Shuffle,
            ..Settings::default()
        };
        settings.add_music_directory("/music".into());
        settings.add_music_directory("/music".into());
        settings.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path).unwrap();
        assert_eq!(loaded.color_theme, "mushan");
        assert!((loaded.volume - 1.0).abs() < f64::EPSILON);
        assert_eq!(loaded.sort_column, SortColumn::Album);
        assert!(!loaded.sort_ascending);
        assert_eq!(loaded.play_mode, PlayMode::Shuffle);
        assert_eq!(loaded.music_directories, vec![PathBuf::from("/music")]);
        assert!(Settings::load_status_from(&path).can_prune_library());
    }

    #[test]
    fn legacy_dark_theme_flag_becomes_a_color_theme() {
        let path = unique_temp_path("legacy-theme.toml");
        fs::write(&path, "version = 1\ndark_theme = true\n").unwrap();
        let loaded = Settings::load_from(&path).unwrap();
        assert_eq!(loaded.color_theme, "songyan");

        fs::write(&path, "version = 1\ncolor_theme = \"nope\"\n").unwrap();
        let unknown = Settings::load_from(&path).unwrap();
        assert_eq!(unknown.color_theme, "qingci");
    }

    #[test]
    fn rejects_invalid_settings_and_resets_unknown_sort() {
        let path = unique_temp_path("invalid.toml");
        fs::write(&path, "not = toml {").unwrap();
        assert!(Settings::load_from(&path).is_err());
        assert!(matches!(
            Settings::load_status_from(&path),
            SettingsLoad::Failed { .. }
        ));
        assert!(!Settings::load_status_from(&path).can_prune_library());

        let settings_path = unique_temp_path("sort.toml");
        fs::write(
            &settings_path,
            "version = 1\nsort_column = \"size\"\nvolume = 2.5\n",
        )
        .unwrap();
        let loaded = Settings::load_from(&settings_path).unwrap();
        assert_eq!(loaded.sort_column, SortColumn::Title);
        assert!(loaded.sort_ascending);
        assert_eq!(loaded.play_mode, PlayMode::Sequential);
        assert!((loaded.volume - 1.0).abs() < f64::EPSILON);

        let mode_path = unique_temp_path("play-mode.toml");
        fs::write(&mode_path, "version = 1\nplay_mode = \"loop-forever\"\n").unwrap();
        let loaded_mode = Settings::load_from(&mode_path).unwrap();
        assert_eq!(loaded_mode.play_mode, PlayMode::Sequential);
    }

    #[test]
    fn refuses_future_settings_versions() {
        let path = unique_temp_path("future.toml");
        fs::write(&path, "version = 99\nvolume = 0.5\n").unwrap();
        match Settings::load_from(&path) {
            Err(SettingsError::UnsupportedVersion {
                found: 99,
                supported: 1,
            }) => {}
            other => panic!("unexpected result {other:?}"),
        }
        let settings = Settings {
            version: 99,
            ..Settings::default()
        };
        assert!(matches!(
            settings.save_to(&path),
            Err(SettingsError::UnsupportedVersion { found: 99, .. })
        ));
    }

    #[test]
    fn keeps_previous_file_when_temp_replace_target_is_a_directory() {
        let directory = unique_temp_dir("atomic");
        let path = directory.join("settings.toml");
        fs::write(&path, "version = 1\nvolume = 0.25\n").unwrap();
        let conflict = unique_temp_dir("atomic-conflict");
        let settings = Settings {
            volume: 0.4,
            ..Settings::default()
        };
        assert!(settings.save_to(&conflict).is_err());
        let original = fs::read_to_string(&path).unwrap();
        assert!(original.contains("0.25"));
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        unique_temp_dir(name).join(name)
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "qingyin-settings-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos(),
            name
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }
}
