use gstreamer as gst;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Paused,
    Playing,
}

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error("GStreamer initialization failed: {0}")]
    Initialization(#[from] gst::glib::Error),
}

#[derive(Debug, Default)]
pub struct Player {
    state: PlaybackState,
}

impl Player {
    /// Initializes `GStreamer` and creates an idle player.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when `GStreamer` cannot be initialized.
    pub fn initialize() -> Result<Self, PlayerError> {
        gst::init()?;
        Ok(Self::default())
    }

    #[must_use]
    pub const fn state(&self) -> PlaybackState {
        self.state
    }
}
