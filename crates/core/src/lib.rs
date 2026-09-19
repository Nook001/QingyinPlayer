use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use qingyin_library::MusicLibrary;
use qingyin_metadata::TrackMetadata;
use qingyin_player::PlaybackState;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    #[error("unable to determine config directory")]
    MissingConfigHome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub music_directories: Vec<PathBuf>,
    #[serde(default)]
    pub dark_theme: bool,
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(default)]
    pub sort_column: String,
    #[serde(default = "default_sort_ascending")]
    pub sort_ascending: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            music_directories: Vec::new(),
            dark_theme: false,
            volume: DEFAULT_VOLUME,
            sort_column: String::new(),
            sort_ascending: true,
        }
    }
}

impl Settings {
    /// Loads settings from the user config path, falling back to defaults.
    #[must_use]
    pub fn load_or_default() -> Self {
        match Self::load() {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "using default settings");
                Self::default()
            }
        }
    }

    /// Loads settings from the default config path.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError`] when the file exists but cannot be read or parsed.
    pub fn load() -> Result<Self, SettingsError> {
        Self::load_from(settings_path()?)
    }

    /// Loads settings from `path`. A missing file yields defaults.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError`] when the file exists but cannot be read or parsed.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self, SettingsError> {
        let path = path.as_ref();
        match fs::read_to_string(path) {
            Ok(text) => {
                let mut settings = toml::from_str::<Self>(&text)?;
                settings.normalize();
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

    /// Writes settings to `path`, creating parent directories as needed.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError`] when the file cannot be serialized or written.
    pub fn save_to(&self, path: impl AsRef<Path>) -> Result<(), SettingsError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| SettingsError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let mut settings = self.clone();
        settings.normalize();
        fs::write(path, toml::to_string_pretty(&settings)?).map_err(|source| SettingsError::Io {
            path: path.to_path_buf(),
            source,
        })
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

    fn normalize(&mut self) {
        self.version = SETTINGS_VERSION;
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            DEFAULT_VOLUME
        };
        if !matches!(
            self.sort_column.as_str(),
            "" | "title" | "album" | "duration"
        ) {
            self.sort_column.clear();
            self.sort_ascending = true;
        }
        self.music_directories
            .retain(|path| !path.as_os_str().is_empty());
    }
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
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or(SettingsError::MissingConfigHome)?;
    Ok(config_home.join("qingyin/settings.toml"))
}

#[derive(Debug, Default)]
pub struct PlaybackQueue {
    tracks: VecDeque<TrackMetadata>,
}

impl PlaybackQueue {
    pub fn push(&mut self, track: TrackMetadata) {
        self.tracks.push_back(track);
    }

    pub fn pop(&mut self) -> Option<TrackMetadata> {
        self.tracks.pop_front()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct AppCore {
    pub library: MusicLibrary,
    pub queue: PlaybackQueue,
    pub playback_state: PlaybackState,
    pub settings: Settings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_file_uses_defaults() {
        let path = unique_temp_path("missing.toml");
        let settings = Settings::load_from(&path).unwrap();
        assert_eq!(settings, Settings::default());
        assert!((settings.volume - 1.0).abs() < f64::EPSILON);
        assert!(!settings.dark_theme);
    }

    #[test]
    fn roundtrips_settings_and_clamps_volume() {
        let path = unique_temp_path("settings.toml");
        let mut settings = Settings {
            dark_theme: true,
            volume: 1.8,
            sort_column: "album".into(),
            sort_ascending: false,
            ..Settings::default()
        };
        settings.add_music_directory("/music".into());
        settings.add_music_directory("/music".into());
        settings.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path).unwrap();
        assert!(loaded.dark_theme);
        assert!((loaded.volume - 1.0).abs() < f64::EPSILON);
        assert_eq!(loaded.sort_column, "album");
        assert!(!loaded.sort_ascending);
        assert_eq!(loaded.music_directories, vec![PathBuf::from("/music")]);
    }

    #[test]
    fn rejects_invalid_settings_and_resets_unknown_sort() {
        let path = unique_temp_path("invalid.toml");
        fs::write(&path, "not = toml {").unwrap();
        assert!(Settings::load_from(&path).is_err());

        let settings_path = unique_temp_path("sort.toml");
        fs::write(
            &settings_path,
            "version = 1\nsort_column = \"size\"\nvolume = 2.5\n",
        )
        .unwrap();
        let loaded = Settings::load_from(&settings_path).unwrap();
        assert!(loaded.sort_column.is_empty());
        assert!(loaded.sort_ascending);
        assert!((loaded.volume - 1.0).abs() < f64::EPSILON);
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("qingyin-settings-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory.join(name)
    }
}
