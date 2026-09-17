use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub path: PathBuf,
    pub title: String,
    pub album: Option<String>,
    pub artists: Vec<String>,
    pub duration: Option<Duration>,
}
