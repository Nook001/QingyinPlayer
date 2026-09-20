use gst::glib;
use gst::prelude::*;
use gstreamer as gst;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use thiserror::Error;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

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
    Progress {
        position: Duration,
        duration: Option<Duration>,
    },
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

    #[error("failed to watch the GStreamer bus")]
    BusWatch,

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
    volume: f64,
    gst_context: Option<glib::MainContext>,
    event_monitor: Option<EventMonitor>,
}

struct EventMonitor {
    main_loop: glib::MainLoop,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for EventMonitor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventMonitor")
            .finish_non_exhaustive()
    }
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
        if let Ok(video_sink) = gst::ElementFactory::make("fakesink").build() {
            video_sink.set_property("sync", false);
            playbin.set_property("video-sink", video_sink);
        }
        if let Ok(text_sink) = gst::ElementFactory::make("fakesink").build() {
            text_sink.set_property("sync", false);
            playbin.set_property("text-sink", text_sink);
        }
        playbin.set_property_from_str("flags", "audio+soft-volume");
        let volume = playbin.property("volume");

        Ok(Self {
            state: PlaybackState::Stopped,
            playbin,
            current_path: None,
            volume,
            gst_context: None,
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
        self.query_clock_safe(QueryClock::Position)
    }

    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        self.query_clock_safe(QueryClock::Duration)
    }

    #[must_use]
    pub const fn volume(&self) -> f64 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.volume = volume.clamp(0.0, 1.0);
        let volume = self.volume;
        self.on_gst_thread(move |playbin| {
            playbin.set_property("volume", volume);
        });
    }

    /// Seeks within the loaded track.
    ///
    /// The seek is queued onto the playback thread. Failures after that
    /// arrive as [`PlayerEvent::Error`].
    ///
    /// # Errors
    ///
    /// Currently always returns `Ok`; the signature keeps [`PlayerError`] for callers.
    pub fn seek(&mut self, position: Duration) -> Result<(), PlayerError> {
        let position =
            gst::ClockTime::from_nseconds(u64::try_from(position.as_nanos()).unwrap_or(u64::MAX));
        self.on_gst_thread(move |playbin| {
            if let Err(error) =
                playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, position)
            {
                tracing::warn!(error = %error, "GStreamer seek failed");
            }
        });
        Ok(())
    }

    /// Starts forwarding bus events and playback progress to `handler`.
    ///
    /// End-of-stream, errors, and position updates are delivered from a `GLib` main
    /// loop thread. Progress ticks run only while the pipeline is `Playing`.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when the playback element has no event bus or the
    /// watch cannot be attached.
    pub fn set_event_handler<F>(&mut self, handler: F) -> Result<(), PlayerError>
    where
        F: Fn(PlayerEvent) + Send + 'static,
    {
        self.gst_context = None;
        self.event_monitor = None;
        let bus = self.playbin.bus().ok_or(PlayerError::MissingBus)?;
        let playbin = self.playbin.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("qingyin-gst-bus".into())
            .spawn(move || {
                run_bus_loop(bus, playbin, handler, ready_tx);
            })
            .map_err(|_| PlayerError::BusWatch)?;
        if let Ok(Ok(main_loop)) = ready_rx.recv() {
            self.gst_context = Some(main_loop.context());
            self.event_monitor = Some(EventMonitor {
                main_loop,
                thread: Some(thread),
            });
            Ok(())
        } else {
            let _ = thread.join();
            Err(PlayerError::BusWatch)
        }
    }

    fn on_gst_thread<F>(&self, func: F)
    where
        F: FnOnce(&gst::Element) + Send + 'static,
    {
        let playbin = self.playbin.clone();
        if let Some(context) = &self.gst_context {
            context.invoke_with_priority(glib::Priority::DEFAULT, move || func(&playbin));
        } else {
            func(&self.playbin);
        }
    }

    fn query_clock_safe(&self, query: QueryClock) -> Option<Duration> {
        let Some(context) = &self.gst_context else {
            return query_clock(&self.playbin, query);
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let playbin = self.playbin.clone();
        context.invoke_with_priority(glib::Priority::DEFAULT, move || {
            let _ = tx.send(query_clock(&playbin, query));
        });
        rx.recv_timeout(Duration::from_millis(100)).ok().flatten()
    }

    /// Loads a local audio file without starting playback.
    ///
    /// Path checks run on the caller thread; pipeline reset is queued onto the
    /// playback thread. Failures after that arrive as [`PlayerEvent::Error`].
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError`] when the path is invalid.
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

        let uri = uri.as_str().to_owned();
        self.on_gst_thread(move |playbin| {
            if let Err(error) = playbin.set_state(gst::State::Null) {
                tracing::warn!(error = %error, "failed to reset playbin before load");
                return;
            }
            playbin.set_property("uri", uri.as_str());
        });
        self.current_path = Some(path);
        self.state = PlaybackState::Stopped;
        Ok(())
    }

    /// Starts or resumes the loaded track.
    ///
    /// The pipeline may still be prerolling when this returns. Failures after that
    /// arrive as [`PlayerEvent::Error`].
    ///
    /// # Errors
    ///
    /// Currently always returns `Ok`; the signature keeps [`PlayerError`] for callers.
    pub fn play(&mut self) -> Result<(), PlayerError> {
        self.on_gst_thread(|playbin| {
            if let Err(error) = playbin.set_state(gst::State::Playing) {
                tracing::warn!(error = %error, "failed to set playbin playing");
            }
        });
        self.state = PlaybackState::Playing;
        Ok(())
    }

    /// Pauses the current track.
    ///
    /// # Errors
    ///
    /// Currently always returns `Ok`; the signature keeps [`PlayerError`] for callers.
    pub fn pause(&mut self) -> Result<(), PlayerError> {
        self.on_gst_thread(|playbin| {
            if let Err(error) = playbin.set_state(gst::State::Paused) {
                tracing::warn!(error = %error, "failed to set playbin paused");
            }
        });
        self.state = PlaybackState::Paused;
        Ok(())
    }

    /// Stops playback while retaining the loaded track.
    ///
    /// # Errors
    ///
    /// Currently always returns `Ok`; the signature keeps [`PlayerError`] for callers.
    pub fn stop(&mut self) -> Result<(), PlayerError> {
        self.on_gst_thread(|playbin| {
            if let Err(error) = playbin.set_state(gst::State::Null) {
                tracing::warn!(error = %error, "failed to reset playbin");
            }
        });
        self.state = PlaybackState::Stopped;
        Ok(())
    }
}

fn run_bus_loop<F>(
    bus: gst::Bus,
    playbin: gst::Element,
    handler: F,
    ready_tx: mpsc::SyncSender<Result<glib::MainLoop, ()>>,
) where
    F: Fn(PlayerEvent) + Send + 'static,
{
    let context = glib::MainContext::new();
    let main_loop = glib::MainLoop::new(Some(&context), false);
    let fail_tx = ready_tx.clone();
    let loop_context = context.clone();
    if context
        .with_thread_default(move || {
            let handler = BusHandler::new(handler);
            let progress_source = Arc::new(Mutex::new(None::<glib::SourceId>));
            let watch = bus.create_watch(Some("qingyin-gst-bus"), glib::Priority::DEFAULT, {
                let playbin = playbin.clone();
                let handler = Arc::clone(&handler);
                let progress_source = Arc::clone(&progress_source);
                let loop_context = loop_context.clone();
                move |_, message| {
                    handle_bus_message(
                        &playbin,
                        message,
                        &handler,
                        &progress_source,
                        &loop_context,
                    );
                    glib::ControlFlow::Continue
                }
            });
            let _watch_id = watch.attach(Some(&loop_context));
            let _ = ready_tx.send(Ok(main_loop.clone()));
            main_loop.run();
            stop_progress(&progress_source, &loop_context);
        })
        .is_err()
    {
        let _ = fail_tx.send(Err(()));
    }
}

fn handle_bus_message(
    playbin: &gst::Element,
    message: &gst::Message,
    handler: &Arc<BusHandler>,
    progress_source: &Mutex<Option<glib::SourceId>>,
    context: &glib::MainContext,
) {
    match message.view() {
        gst::MessageView::Eos(_) => handler.emit(PlayerEvent::EndOfStream),
        gst::MessageView::Error(error) => {
            handler.emit(PlayerEvent::Error(format_bus_error(error)));
        }
        gst::MessageView::StateChanged(changed) if is_playbin_message(playbin, message) => {
            if changed.current() == gst::State::Playing {
                emit_progress(playbin, handler);
                start_progress(progress_source, context, playbin, handler);
            } else {
                stop_progress(progress_source, context);
            }
        }
        gst::MessageView::DurationChanged(_) if is_playbin_message(playbin, message) => {
            emit_progress(playbin, handler);
        }
        _ => {}
    }
}

fn start_progress(
    progress_source: &Mutex<Option<glib::SourceId>>,
    context: &glib::MainContext,
    playbin: &gst::Element,
    handler: &Arc<BusHandler>,
) {
    let mut slot = progress_source
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if slot.is_some() {
        return;
    }
    let playbin = playbin.clone();
    let handler = Arc::clone(handler);
    let source = glib::timeout_source_new(
        PROGRESS_INTERVAL,
        Some("qingyin-gst-progress"),
        glib::Priority::DEFAULT,
        move || {
            emit_progress(&playbin, &handler);
            glib::ControlFlow::Continue
        },
    );
    *slot = Some(source.attach(Some(context)));
}

fn stop_progress(progress_source: &Mutex<Option<glib::SourceId>>, context: &glib::MainContext) {
    let mut slot = progress_source
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(source_id) = slot.take()
        && let Some(source) = context.find_source_by_id(&source_id)
    {
        source.destroy();
    }
}

fn emit_progress(playbin: &gst::Element, handler: &BusHandler) {
    let Some(position) = query_clock(playbin, QueryClock::Position) else {
        return;
    };
    handler.emit(PlayerEvent::Progress {
        position,
        duration: query_clock(playbin, QueryClock::Duration),
    });
}

struct BusHandler {
    inner: Mutex<Box<dyn Fn(PlayerEvent) + Send>>,
}

impl std::fmt::Debug for BusHandler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("BusHandler").finish_non_exhaustive()
    }
}

impl BusHandler {
    fn new<F>(handler: F) -> Arc<Self>
    where
        F: Fn(PlayerEvent) + Send + 'static,
    {
        Arc::new(Self {
            inner: Mutex::new(Box::new(handler)),
        })
    }

    fn emit(&self, event: PlayerEvent) {
        let handler = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        handler(event);
    }
}

fn is_playbin_message(playbin: &gst::Element, message: &gst::Message) -> bool {
    message
        .src()
        .is_some_and(|src| *src == *playbin.upcast_ref::<gst::Object>())
}

#[derive(Clone, Copy)]
enum QueryClock {
    Position,
    Duration,
}

fn query_clock(playbin: &gst::Element, query: QueryClock) -> Option<Duration> {
    let clock_time = match query {
        QueryClock::Position => playbin.query_position::<gst::ClockTime>(),
        QueryClock::Duration => playbin.query_duration::<gst::ClockTime>(),
    }?;
    Some(Duration::from_nanos(clock_time.nseconds()))
}

fn format_bus_error(error: &gst::message::Error) -> String {
    format_error_parts(&error.error().to_string(), error.debug().as_deref())
}

fn format_error_parts(error: &str, debug: Option<&str>) -> String {
    match debug {
        Some(debug) if !debug.is_empty() => format!("{error}: {debug}"),
        _ => error.to_owned(),
    }
}

fn ensure_format_plugins(path: &Path) -> Result<(), PlayerError> {
    let missing = required_format_plugins(path)
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

fn required_format_plugins(path: &Path) -> &'static [&'static str] {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("flac") => &["flacparse", "flacdec"],
        Some("mp3") => &["id3demux", "mpegaudioparse"],
        _ => &[],
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if let Some(context) = self.gst_context.take() {
            let playbin = self.playbin.clone();
            let (tx, rx) = mpsc::sync_channel(1);
            context.invoke_with_priority(glib::Priority::DEFAULT, move || {
                let _ = playbin.set_state(gst::State::Null);
                let _ = tx.send(());
            });
            let _ = rx.recv_timeout(Duration::from_secs(2));
        } else {
            let _ = self.playbin.set_state(gst::State::Null);
        }
        self.event_monitor = None;
    }
}

impl Drop for EventMonitor {
    fn drop(&mut self) {
        self.main_loop.quit();
        self.main_loop.context().wakeup();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_extensions_to_required_plugins() {
        assert_eq!(
            required_format_plugins(Path::new("/music/a.FLAC")),
            ["flacparse", "flacdec"]
        );
        assert_eq!(
            required_format_plugins(Path::new("track.mp3")),
            ["id3demux", "mpegaudioparse"]
        );
        assert!(required_format_plugins(Path::new("track.wav")).is_empty());
        assert!(required_format_plugins(Path::new("track")).is_empty());
    }

    #[test]
    fn unnamed_formats_do_not_require_named_plugins() {
        gst::init().expect("GStreamer should initialize in tests");
        assert!(ensure_format_plugins(Path::new("track.ogg")).is_ok());
        assert!(ensure_format_plugins(Path::new("track.wav")).is_ok());
    }

    #[test]
    fn formats_bus_errors_with_optional_debug() {
        assert_eq!(format_error_parts("decode failed", None), "decode failed");
        assert_eq!(
            format_error_parts("decode failed", Some("")),
            "decode failed"
        );
        assert_eq!(
            format_error_parts("decode failed", Some("no decoder")),
            "decode failed: no decoder"
        );
    }

    #[test]
    fn missing_plugin_error_lists_element_names() {
        let error = PlayerError::MissingPlugins {
            path: PathBuf::from("/music/a.flac"),
            plugins: "flacparse, flacdec".into(),
        };
        let message = error.to_string();
        assert!(message.contains("flacparse"));
        assert!(message.contains("/music/a.flac"));
    }

    #[test]
    fn dropping_a_player_stops_the_bus_loop() {
        let mut player = Player::initialize().expect("GStreamer should initialize");
        player
            .set_event_handler(|_| {})
            .expect("bus loop should start");
        drop(player);
    }
}
