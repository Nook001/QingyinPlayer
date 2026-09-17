use gst::prelude::*;
use gstreamer as gst;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Paused,
    Playing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerEvent {
    EndOfStream,
    Error(String),
}

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error("GStreamer initialization failed: {0}")]
    Initialization(#[from] gst::glib::Error),

    #[error("failed to create GStreamer playbin: {0}")]
    CreatePlaybin(gst::glib::BoolError),

    #[error("audio file does not exist or is not a regular file: {}", path.display())]
    InvalidPath { path: PathBuf },

    #[error("failed to convert audio path to URI ({path:?}): {source}")]
    InvalidUri {
        path: PathBuf,
        source: gst::glib::Error,
    },

    #[error("required GStreamer plugins are missing for {}: {plugins}", path.display())]
    MissingPlugins { path: PathBuf, plugins: String },

    #[error("GStreamer playbin does not provide an event bus")]
    MissingBus,

    #[error("GStreamer state change failed: {0}")]
    StateChange(String),

    #[error("GStreamer seek failed: {0}")]
    Seek(String),
}

#[derive(Debug)]
pub struct Player {
    state: PlaybackState,
    playbin: gst::Element,
    current_path: Option<PathBuf>,
    event_monitor: Option<EventMonitor>,
}

#[derive(Debug)]
struct EventMonitor {
    stop_requested: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// Initializes `GStreamer` and creates an idle player.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when `GStreamer` cannot be initialized.
    pub fn initialize() -> Result<Self, PlayerError> {
        gst::init()?;
        let playbin = gst::ElementFactory::make("playbin")
            .build()
            .map_err(PlayerError::CreatePlaybin)?;
        if gst::ElementFactory::find("autoaudiosink").is_none()
            && let Ok(audio_sink) = gst::ElementFactory::make("pipewiresink").build()
        {
            playbin.set_property("audio-sink", audio_sink);
        }

        Ok(Self {
            state: PlaybackState::Stopped,
            playbin,
            current_path: None,
            event_monitor: None,
        })
    }

    #[must_use]
    pub const fn state(&self) -> PlaybackState {
        self.state
    }

    #[must_use]
    pub fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    #[must_use]
    pub fn position(&self) -> Option<Duration> {
        self.playbin
            .query_position::<gst::ClockTime>()
            .map(|position| Duration::from_nanos(position.nseconds()))
    }

    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        self.playbin
            .query_duration::<gst::ClockTime>()
            .map(|duration| Duration::from_nanos(duration.nseconds()))
    }

    #[must_use]
    pub fn volume(&self) -> f64 {
        self.playbin.property("volume")
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.playbin.set_property("volume", volume.clamp(0.0, 1.0));
    }

    /// Seeks within the loaded track.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when `GStreamer` rejects the seek operation.
    pub fn seek(&mut self, position: Duration) -> Result<(), PlayerError> {
        let position =
            gst::ClockTime::from_nseconds(u64::try_from(position.as_nanos()).unwrap_or(u64::MAX));
        self.playbin
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, position)
            .map_err(|error| PlayerError::Seek(error.to_string()))
    }

    /// Starts forwarding end-of-stream and playback errors to `handler`.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when the playback element has no event bus.
    pub fn set_event_handler<F>(&mut self, handler: F) -> Result<(), PlayerError>
    where
        F: Fn(PlayerEvent) + Send + 'static,
    {
        self.event_monitor = None;
        let bus = self.playbin.bus().ok_or(PlayerError::MissingBus)?;
        let stop_requested = Arc::new(AtomicBool::new(false));
        let monitor_stop = Arc::clone(&stop_requested);
        let thread = std::thread::spawn(move || {
            while !monitor_stop.load(Ordering::Relaxed) {
                let Some(message) = bus.timed_pop(Some(gst::ClockTime::from_mseconds(250))) else {
                    continue;
                };
                match message.view() {
                    gst::MessageView::Eos(_) => handler(PlayerEvent::EndOfStream),
                    gst::MessageView::Error(error) => {
                        handler(PlayerEvent::Error(format_bus_error(error)));
                    }
                    _ => {}
                }
            }
        });
        self.event_monitor = Some(EventMonitor {
            stop_requested,
            thread: Some(thread),
        });
        Ok(())
    }

    /// Loads a local audio file without starting playback.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when the path is invalid or `GStreamer` rejects the state change.
    pub fn load(&mut self, path: impl AsRef<Path>) -> Result<(), PlayerError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(PlayerError::InvalidPath {
                path: path.to_path_buf(),
            });
        }

        let path = path.canonicalize().map_err(|_| PlayerError::InvalidPath {
            path: path.to_path_buf(),
        })?;
        ensure_format_plugins(&path)?;
        let uri =
            gst::glib::filename_to_uri(&path, None).map_err(|source| PlayerError::InvalidUri {
                path: path.clone(),
                source,
            })?;

        self.set_gstreamer_state(gst::State::Null)?;
        self.playbin.set_property("uri", uri.as_str());
        self.current_path = Some(path);
        self.state = PlaybackState::Stopped;
        Ok(())
    }

    /// Starts or resumes the loaded track.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when `GStreamer` rejects the state change.
    pub fn play(&mut self) -> Result<(), PlayerError> {
        self.set_gstreamer_state(gst::State::Playing)?;
        self.state = PlaybackState::Playing;
        Ok(())
    }

    /// Pauses the current track.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when `GStreamer` rejects the state change.
    pub fn pause(&mut self) -> Result<(), PlayerError> {
        self.set_gstreamer_state(gst::State::Paused)?;
        self.state = PlaybackState::Paused;
        Ok(())
    }

    /// Stops playback while retaining the loaded track.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when `GStreamer` rejects the state change.
    pub fn stop(&mut self) -> Result<(), PlayerError> {
        self.set_gstreamer_state(gst::State::Null)?;
        self.state = PlaybackState::Stopped;
        Ok(())
    }

    fn set_gstreamer_state(&self, state: gst::State) -> Result<(), PlayerError> {
        self.playbin
            .set_state(state)
            .map(|_| ())
            .map_err(|error| PlayerError::StateChange(error.to_string()))?;

        if matches!(state, gst::State::Playing | gst::State::Paused) {
            let (result, _, _) = self.playbin.state(Some(gst::ClockTime::from_seconds(3)));
            result.map_err(|error| self.playback_error(error))?;
        }
        Ok(())
    }

    fn playback_error(&self, state_error: gst::StateChangeError) -> PlayerError {
        let Some(bus) = self.playbin.bus() else {
            return PlayerError::StateChange(state_error.to_string());
        };
        let mut details = Vec::new();

        while let Some(message) = bus.pop() {
            if message.has_name("missing-plugin") {
                if let Some(structure) = message.structure() {
                    details.push(format!("missing GStreamer plugin: {structure}"));
                }
            } else if let gst::MessageView::Error(error) = message.view() {
                let mut message = error.error().to_string();
                if let Some(debug) = error.debug() {
                    message.push_str(": ");
                    message.push_str(&debug);
                }
                details.push(message);
            }
        }

        if details.is_empty() {
            PlayerError::StateChange(state_error.to_string())
        } else {
            PlayerError::StateChange(details.join("; "))
        }
    }
}

fn format_bus_error(error: &gst::message::Error) -> String {
    let mut message = error.error().to_string();
    if let Some(debug) = error.debug() {
        message.push_str(": ");
        message.push_str(&debug);
    }
    message
}

fn ensure_format_plugins(path: &Path) -> Result<(), PlayerError> {
    let required = match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("flac") => &["flacparse", "flacdec"][..],
        Some("mp3") => &["id3demux", "mpegaudioparse"][..],
        _ => &[],
    };
    let missing = required
        .iter()
        .filter(|plugin| gst::ElementFactory::find(plugin).is_none())
        .copied()
        .collect::<Vec<_>>();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(PlayerError::MissingPlugins {
            path: path.to_path_buf(),
            plugins: missing.join(", "),
        })
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.playbin.set_state(gst::State::Null);
    }
}

impl Drop for EventMonitor {
    fn drop(&mut self) {
        self.stop_requested.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
