use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathError {
    #[error("unable to determine home directory")]
    MissingHome,
}

/// XDG config, data, and cache directories for Qingyin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdgDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

impl XdgDirs {
    /// Resolves Qingyin directories from the environment.
    ///
    /// Absolute `XDG_*_HOME` values win. Empty or relative overrides are ignored so callers
    /// cannot accidentally write next to the process working directory.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::MissingHome`] when neither a valid override nor `HOME` is available.
    pub fn resolve() -> Result<Self, PathError> {
        Ok(Self {
            config: qingyin_dir("XDG_CONFIG_HOME", ".config")?,
            data: qingyin_dir("XDG_DATA_HOME", ".local/share")?,
            cache: qingyin_dir("XDG_CACHE_HOME", ".cache")?,
        })
    }

    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.toml")
    }

    #[must_use]
    pub fn database_file(&self) -> PathBuf {
        self.data.join("library.sqlite3")
    }

    #[must_use]
    pub fn cover_cache_dir(&self) -> PathBuf {
        self.cache.join("covers")
    }
}

fn qingyin_dir(override_var: &str, home_suffix: &str) -> Result<PathBuf, PathError> {
    if let Some(value) = std::env::var_os(override_var) {
        let path = PathBuf::from(value);
        if is_usable_override(&path) {
            return Ok(path.join("qingyin"));
        }
    }
    let home = std::env::var_os("HOME").ok_or(PathError::MissingHome)?;
    Ok(PathBuf::from(home).join(home_suffix).join("qingyin"))
}

fn is_usable_override(path: &Path) -> bool {
    path.is_absolute() && !path.as_os_str().is_empty()
}

#[cfg(test)]
mod tests {
    use super::is_usable_override;
    use std::path::Path;

    #[test]
    fn rejects_empty_and_relative_overrides() {
        assert!(!is_usable_override(Path::new("")));
        assert!(!is_usable_override(Path::new("relative/cache")));
        assert!(is_usable_override(Path::new("/tmp/qingyin-cache")));
    }
}
