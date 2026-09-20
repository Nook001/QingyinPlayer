use gst::glib;
use gst::prelude::*;
use gstreamer as gst;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use thiserror::Error;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
const HIDDEN_PROGRESS_INTERVAL: Duration = Duration::from_secs(2);
const DURATION_PROBE_LIMIT: u32 = 8;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

pub type CommandId = u64;
pub type LoadGeneration = u64;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Paused,
    Playing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Prepare,
    Load,
    Play,
    Pause,
    Stop,
    Seek,
    SetVolume,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    Ready,
    CommandFinished {
        id: CommandId,
        kind: CommandKind,
        error: Option<String>,
    },
    StateChanged {
        generation: LoadGeneration,
        state: PlaybackState,
    },
    Progress {
        generation: LoadGeneration,
        position: Duration,
        duration: Option<Duration>,
    },
    EndOfStream {
        generation: LoadGeneration,
    },
    Error {
        generation: LoadGeneration,
        message: String,
    },
    ShutdownFinished,
}

#[derive(Debug, Clone)]
enum PlayerCommand {
    Prepare,
    Load {
        path: PathBuf,
        generation: LoadGeneration,
    },
    Play,
    Pause,
    Stop,
    Seek(Duration),
    SetVolume(f64),
    SetProgressInterval(Duration),
    Shutdown,
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
    #[error("playback backend is not ready")]
    NotReady,
    #[error("playback backend shutdown timed out")]
    ShutdownTimeout,
    #[error("playback command channel closed")]
    Disconnected,
}

/// Frontend that assigns command IDs and load generations. Confirmed state comes from events.
#[derive(Debug)]
pub struct Player {
    commands: Sender<(CommandId, PlayerCommand)>,
    next_id: CommandId,
    generation: LoadGeneration,
    requested: PlaybackState,
    confirmed: PlaybackState,
    current_path: Option<PathBuf>,
    volume: f64,
    ready: bool,
    shutdown: Option<Receiver<()>>,
    worker: Option<JoinHandle<()>>,
}

impl Player {
    /// Starts GStreamer on a dedicated thread and reports [`PlayerEvent::Ready`] when prepared.
    pub fn spawn(handler: impl Fn(PlayerEvent) + Send + Sync + 'static) -> Self {
        spawn_backend(BackendKind::GStreamer, handler)
    }

    /// Starts a deterministic fake backend for controller tests.
    pub fn spawn_fake(handler: impl Fn(PlayerEvent) + Send + Sync + 'static) -> (Self, FakeHandle) {
        let (inject_tx, inject_rx) = mpsc::channel();
        let missing = Arc::new(Mutex::new(HashMap::<PathBuf, bool>::new()));
        let fail_next = Arc::new(Mutex::new(None::<(CommandKind, String)>));
        let player = spawn_backend(
            BackendKind::Fake {
                inject: inject_rx,
                missing: Arc::clone(&missing),
                fail_next: Arc::clone(&fail_next),
            },
            handler,
        );
        (
            player,
            FakeHandle {
                inject: inject_tx,
                missing,
                fail_next,
            },
        )
    }

    #[must_use]
    pub const fn requested_state(&self) -> PlaybackState {
        self.requested
    }

    #[must_use]
    pub const fn confirmed_state(&self) -> PlaybackState {
        self.confirmed
    }

    #[must_use]
    pub const fn generation(&self) -> LoadGeneration {
        self.generation
    }

    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.ready
    }

    #[must_use]
    pub fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    #[must_use]
    pub const fn volume(&self) -> f64 {
        self.volume
    }

    pub fn mark_ready(&mut self) {
        self.ready = true;
    }

    pub fn apply_confirmed_state(&mut self, generation: LoadGeneration, state: PlaybackState) {
        if generation == self.generation {
            self.confirmed = state;
        }
    }

    pub fn prepare(&mut self) -> CommandId {
        self.submit(CommandKind::Prepare, PlayerCommand::Prepare)
    }

    pub fn load(&mut self, path: impl AsRef<Path>) -> CommandId {
        self.generation = self.generation.wrapping_add(1);
        self.requested = PlaybackState::Stopped;
        self.confirmed = PlaybackState::Stopped;
        self.current_path = Some(path.as_ref().to_path_buf());
        self.submit(
            CommandKind::Load,
            PlayerCommand::Load {
                path: path.as_ref().to_path_buf(),
                generation: self.generation,
            },
        )
    }

    pub fn play(&mut self) -> CommandId {
        self.requested = PlaybackState::Playing;
        self.submit(CommandKind::Play, PlayerCommand::Play)
    }

    pub fn pause(&mut self) -> CommandId {
        self.requested = PlaybackState::Paused;
        self.submit(CommandKind::Pause, PlayerCommand::Pause)
    }

    pub fn stop(&mut self) -> CommandId {
        self.requested = PlaybackState::Stopped;
        self.submit(CommandKind::Stop, PlayerCommand::Stop)
    }

    pub fn seek(&mut self, position: Duration) -> CommandId {
        self.submit(CommandKind::Seek, PlayerCommand::Seek(position))
    }

    pub fn set_volume(&mut self, volume: f64) -> CommandId {
        self.volume = volume.clamp(0.0, 1.0);
        self.submit(
            CommandKind::SetVolume,
            PlayerCommand::SetVolume(self.volume),
        )
    }

    pub fn set_ui_visible(&mut self, visible: bool) {
        let interval = if visible {
            PROGRESS_INTERVAL
        } else {
            HIDDEN_PROGRESS_INTERVAL
        };
        self.next_id = self.next_id.wrapping_add(1);
        let _ = self
            .commands
            .send((self.next_id, PlayerCommand::SetProgressInterval(interval)));
    }

    /// Waits for shutdown with a bounded timeout. Does not join again after a timeout.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerError::ShutdownTimeout`] when the backend does not acknowledge shutdown.
    pub fn shutdown(&mut self) -> Result<(), PlayerError> {
        let _ = self.submit(CommandKind::Shutdown, PlayerCommand::Shutdown);
        let Some(rx) = self.shutdown.take() else {
            return Ok(());
        };
        match rx.recv_timeout(SHUTDOWN_TIMEOUT) {
            Ok(()) => {
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
                Ok(())
            }
            Err(_) => {
                self.worker.take();
                Err(PlayerError::ShutdownTimeout)
            }
        }
    }

    fn submit(&mut self, kind: CommandKind, command: PlayerCommand) -> CommandId {
        let _ = kind;
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        let _ = self.commands.send((id, command));
        id
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// Test handle for injecting stale events and missing-file answers.
#[derive(Debug, Clone)]
pub struct FakeHandle {
    inject: Sender<PlayerEvent>,
    missing: Arc<Mutex<HashMap<PathBuf, bool>>>,
    fail_next: Arc<Mutex<Option<(CommandKind, String)>>>,
}

impl FakeHandle {
    pub fn inject(&self, event: PlayerEvent) {
        let _ = self.inject.send(event);
    }

    pub fn set_missing(&self, path: impl AsRef<Path>, missing: bool) {
        self.missing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(path.as_ref().to_path_buf(), missing);
    }

    pub fn fail_next(&self, kind: CommandKind, error: impl Into<String>) {
        *self
            .fail_next
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((kind, error.into()));
    }
}

enum BackendKind {
    GStreamer,
    Fake {
        inject: Receiver<PlayerEvent>,
        missing: Arc<Mutex<HashMap<PathBuf, bool>>>,
        fail_next: Arc<Mutex<Option<(CommandKind, String)>>>,
    },
}

fn spawn_backend(
    kind: BackendKind,
    handler: impl Fn(PlayerEvent) + Send + Sync + 'static,
) -> Player {
    let (command_tx, command_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("qingyin-player".into())
        .spawn(move || match kind {
            BackendKind::GStreamer => run_gst_backend(command_rx, handler, done_tx),
            BackendKind::Fake {
                inject,
                missing,
                fail_next,
            } => run_fake_backend(command_rx, inject, missing, fail_next, handler, done_tx),
        })
        .ok();
    Player {
        commands: command_tx,
        next_id: 0,
        generation: 0,
        requested: PlaybackState::Stopped,
        confirmed: PlaybackState::Stopped,
        current_path: None,
        volume: 1.0,
        ready: false,
        shutdown: Some(done_rx),
        worker,
    }
}

fn run_fake_backend(
    commands: Receiver<(CommandId, PlayerCommand)>,
    inject: Receiver<PlayerEvent>,
    missing: Arc<Mutex<HashMap<PathBuf, bool>>>,
    fail_next: Arc<Mutex<Option<(CommandKind, String)>>>,
    handler: impl Fn(PlayerEvent),
    done: SyncSender<()>,
) {
    let mut generation = 0;
    let mut interval = PROGRESS_INTERVAL;
    handler(PlayerEvent::Ready);
    loop {
        if let Ok(event) = inject.try_recv() {
            handler(event);
        }
        match commands.recv_timeout(Duration::from_millis(20)) {
            Ok((id, command)) => {
                let kind = command_kind(&command);
                if let Some((fail_kind, error)) = fail_next
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                    && fail_kind == kind
                {
                    handler(PlayerEvent::CommandFinished {
                        id,
                        kind,
                        error: Some(error.clone()),
                    });
                    if matches!(kind, CommandKind::Load | CommandKind::Play) {
                        handler(PlayerEvent::Error {
                            generation,
                            message: error,
                        });
                    }
                    continue;
                }
                match command {
                    PlayerCommand::Prepare => handler(PlayerEvent::CommandFinished {
                        id,
                        kind,
                        error: None,
                    }),
                    PlayerCommand::Load {
                        path,
                        generation: next,
                    } => {
                        generation = next;
                        let is_missing = missing
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .get(&path)
                            .copied()
                            .unwrap_or_else(|| !path.is_file());
                        if is_missing {
                            let error = format!("missing {}", path.display());
                            handler(PlayerEvent::CommandFinished {
                                id,
                                kind,
                                error: Some(error.clone()),
                            });
                            handler(PlayerEvent::Error {
                                generation,
                                message: error,
                            });
                            handler(PlayerEvent::StateChanged {
                                generation,
                                state: PlaybackState::Stopped,
                            });
                        } else {
                            handler(PlayerEvent::CommandFinished {
                                id,
                                kind,
                                error: None,
                            });
                            handler(PlayerEvent::StateChanged {
                                generation,
                                state: PlaybackState::Stopped,
                            });
                        }
                    }
                    PlayerCommand::Play => {
                        handler(PlayerEvent::CommandFinished {
                            id,
                            kind,
                            error: None,
                        });
                        handler(PlayerEvent::StateChanged {
                            generation,
                            state: PlaybackState::Playing,
                        });
                    }
                    PlayerCommand::Pause => {
                        handler(PlayerEvent::CommandFinished {
                            id,
                            kind,
                            error: None,
                        });
                        handler(PlayerEvent::StateChanged {
                            generation,
                            state: PlaybackState::Paused,
                        });
                    }
                    PlayerCommand::Stop => {
                        handler(PlayerEvent::CommandFinished {
                            id,
                            kind,
                            error: None,
                        });
                        handler(PlayerEvent::StateChanged {
                            generation,
                            state: PlaybackState::Stopped,
                        });
                    }
                    PlayerCommand::Seek(position) => {
                        handler(PlayerEvent::CommandFinished {
                            id,
                            kind,
                            error: None,
                        });
                        handler(PlayerEvent::Progress {
                            generation,
                            position,
                            duration: Some(Duration::from_secs(180)),
                        });
                    }
                    PlayerCommand::SetVolume(_) => handler(PlayerEvent::CommandFinished {
                        id,
                        kind,
                        error: None,
                    }),
                    PlayerCommand::SetProgressInterval(next) => interval = next,
                    PlayerCommand::Shutdown => {
                        handler(PlayerEvent::CommandFinished {
                            id,
                            kind,
                            error: None,
                        });
                        handler(PlayerEvent::ShutdownFinished);
                        let _ = done.send(());
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = interval;
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = done.send(());
                return;
            }
        }
    }
}

fn command_kind(command: &PlayerCommand) -> CommandKind {
    match command {
        PlayerCommand::Prepare => CommandKind::Prepare,
        PlayerCommand::Load { .. } => CommandKind::Load,
        PlayerCommand::Play => CommandKind::Play,
        PlayerCommand::Pause => CommandKind::Pause,
        PlayerCommand::Stop => CommandKind::Stop,
        PlayerCommand::Seek(_) => CommandKind::Seek,
        PlayerCommand::SetVolume(_) | PlayerCommand::SetProgressInterval(_) => {
            CommandKind::SetVolume
        }
        PlayerCommand::Shutdown => CommandKind::Shutdown,
    }
}

fn run_gst_backend(
    commands: Receiver<(CommandId, PlayerCommand)>,
    handler: impl Fn(PlayerEvent) + Send + Sync + 'static,
    done: SyncSender<()>,
) {
    let handler: Arc<dyn Fn(PlayerEvent) + Send + Sync> = Arc::new(handler);
    if let Err(error) = gst::init() {
        handler(PlayerEvent::Error {
            generation: 0,
            message: error.to_string(),
        });
        let _ = done.send(());
        return;
    }
    let playbin = match gst::ElementFactory::make("playbin").build() {
        Ok(playbin) => playbin,
        Err(error) => {
            handler(PlayerEvent::Error {
                generation: 0,
                message: error.to_string(),
            });
            let _ = done.send(());
            return;
        }
    };
    configure_playbin(&playbin);
    let Some(bus) = playbin.bus() else {
        handler(PlayerEvent::Error {
            generation: 0,
            message: PlayerError::MissingBus.to_string(),
        });
        let _ = done.send(());
        return;
    };
    let context = glib::MainContext::new();
    let main_loop = glib::MainLoop::new(Some(&context), false);
    let generation = Arc::new(Mutex::new(0_u64));
    let known_duration = Arc::new(Mutex::new(None::<Duration>));
    let progress_source = Arc::new(Mutex::new(None::<glib::SourceId>));
    let progress_interval = Arc::new(Mutex::new(PROGRESS_INTERVAL));
    let duration_probes = Arc::new(Mutex::new(0_u32));
    let _ = context.with_thread_default(|| {
        let watch = bus.create_watch(Some("qingyin-gst-bus"), glib::Priority::DEFAULT, {
            let playbin = playbin.clone();
            let handler = Arc::clone(&handler);
            let generation = Arc::clone(&generation);
            let known_duration = Arc::clone(&known_duration);
            let progress_source = Arc::clone(&progress_source);
            let progress_interval = Arc::clone(&progress_interval);
            let duration_probes = Arc::clone(&duration_probes);
            let loop_context = context.clone();
            move |_, message| {
                handle_bus_message(
                    &playbin,
                    message,
                    &handler,
                    &generation,
                    &known_duration,
                    &progress_source,
                    &progress_interval,
                    &duration_probes,
                    &loop_context,
                );
                glib::ControlFlow::Continue
            }
        });
        let _watch_id = watch.attach(Some(&context));
        let command_source = glib::timeout_source_new(
            Duration::from_millis(10),
            Some("qingyin-gst-commands"),
            glib::Priority::DEFAULT,
            {
                let playbin = playbin.clone();
                let handler = Arc::clone(&handler);
                let generation = Arc::clone(&generation);
                let known_duration = Arc::clone(&known_duration);
                let progress_source = Arc::clone(&progress_source);
                let progress_interval = Arc::clone(&progress_interval);
                let duration_probes = Arc::clone(&duration_probes);
                let loop_context = context.clone();
                let main_loop = main_loop.clone();
                let commands = Arc::new(Mutex::new(commands));
                let done = done.clone();
                move || {
                    let mut stopping = false;
                    loop {
                        let received = commands
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .try_recv();
                        match received {
                            Ok((id, command)) => {
                                if matches!(command, PlayerCommand::Shutdown) {
                                    execute_gst_command(
                                        id,
                                        command,
                                        &playbin,
                                        &handler,
                                        &generation,
                                        &known_duration,
                                        &progress_source,
                                        &progress_interval,
                                        &duration_probes,
                                        &loop_context,
                                    );
                                    stopping = true;
                                    break;
                                }
                                execute_gst_command(
                                    id,
                                    command,
                                    &playbin,
                                    &handler,
                                    &generation,
                                    &known_duration,
                                    &progress_source,
                                    &progress_interval,
                                    &duration_probes,
                                    &loop_context,
                                );
                            }
                            Err(_) => break,
                        }
                    }
                    if stopping {
                        let _ = playbin.set_state(gst::State::Null);
                        stop_progress(&progress_source, &loop_context);
                        handler(PlayerEvent::ShutdownFinished);
                        let _ = done.send(());
                        main_loop.quit();
                        glib::ControlFlow::Break
                    } else {
                        glib::ControlFlow::Continue
                    }
                }
            },
        );
        let _command_id = command_source.attach(Some(&context));
        handler(PlayerEvent::Ready);
        main_loop.run();
    });
}

fn configure_playbin(playbin: &gst::Element) {
    let use_fake_audio = cfg!(test)
        || std::env::var_os("QINGYIN_AUDIO_SINK").is_some_and(|value| value == "fakesink");
    if use_fake_audio {
        if let Ok(audio_sink) = gst::ElementFactory::make("fakesink").build() {
            audio_sink.set_property("sync", true);
            playbin.set_property("audio-sink", audio_sink);
        }
    } else if gst::ElementFactory::find("autoaudiosink").is_none()
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
}

#[allow(clippy::too_many_arguments)]
fn execute_gst_command(
    id: CommandId,
    command: PlayerCommand,
    playbin: &gst::Element,
    handler: &Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    generation: &Mutex<LoadGeneration>,
    known_duration: &Mutex<Option<Duration>>,
    progress_source: &Mutex<Option<glib::SourceId>>,
    progress_interval: &Mutex<Duration>,
    duration_probes: &Mutex<u32>,
    context: &glib::MainContext,
) {
    let kind = command_kind(&command);
    let result = match command {
        PlayerCommand::Prepare => Ok(()),
        PlayerCommand::Load {
            path,
            generation: next,
        } => {
            *generation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
            *known_duration
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            *duration_probes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = 0;
            stop_progress(progress_source, context);
            load_on_backend(playbin, &path, handler, next)
        }
        PlayerCommand::Play => set_state(playbin, gst::State::Playing).map(|()| {
            start_progress(
                progress_source,
                context,
                playbin,
                handler,
                generation,
                known_duration,
                progress_interval,
                duration_probes,
            );
        }),
        PlayerCommand::Pause => {
            stop_progress(progress_source, context);
            set_state(playbin, gst::State::Paused)
        }
        PlayerCommand::Stop => {
            stop_progress(progress_source, context);
            set_state(playbin, gst::State::Null)
        }
        PlayerCommand::Seek(position) => seek_on_backend(playbin, position),
        PlayerCommand::SetVolume(volume) => {
            playbin.set_property("volume", volume);
            Ok(())
        }
        PlayerCommand::SetProgressInterval(interval) => {
            *progress_interval
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = interval;
            let restart = progress_source
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_some();
            if restart {
                stop_progress(progress_source, context);
                start_progress(
                    progress_source,
                    context,
                    playbin,
                    handler,
                    generation,
                    known_duration,
                    progress_interval,
                    duration_probes,
                );
            }
            Ok(())
        }
        PlayerCommand::Shutdown => Ok(()),
    };
    handler(PlayerEvent::CommandFinished {
        id,
        kind,
        error: result.err().map(|error| error.to_string()),
    });
}

fn load_on_backend(
    playbin: &gst::Element,
    path: &Path,
    handler: &Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    generation: LoadGeneration,
) -> Result<(), PlayerError> {
    if !path.is_file() {
        let error = PlayerError::InvalidPath {
            path: path.to_path_buf(),
        };
        handler(PlayerEvent::Error {
            generation,
            message: error.to_string(),
        });
        let _ = playbin.set_state(gst::State::Null);
        handler(PlayerEvent::StateChanged {
            generation,
            state: PlaybackState::Stopped,
        });
        return Err(error);
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
    if let Err(error) = playbin.set_state(gst::State::Null) {
        let error = PlayerError::StateChange(error.to_string());
        handler(PlayerEvent::Error {
            generation,
            message: error.to_string(),
        });
        handler(PlayerEvent::StateChanged {
            generation,
            state: PlaybackState::Stopped,
        });
        return Err(error);
    }
    playbin.set_property("uri", uri.as_str());
    handler(PlayerEvent::StateChanged {
        generation,
        state: PlaybackState::Stopped,
    });
    Ok(())
}

fn set_state(playbin: &gst::Element, state: gst::State) -> Result<(), PlayerError> {
    playbin
        .set_state(state)
        .map(|_| ())
        .map_err(|error| PlayerError::StateChange(error.to_string()))
}

fn seek_on_backend(playbin: &gst::Element, position: Duration) -> Result<(), PlayerError> {
    let position =
        gst::ClockTime::from_nseconds(u64::try_from(position.as_nanos()).unwrap_or(u64::MAX));
    playbin
        .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, position)
        .map_err(|error| PlayerError::Seek(error.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn handle_bus_message(
    playbin: &gst::Element,
    message: &gst::Message,
    handler: &Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    generation: &Mutex<LoadGeneration>,
    known_duration: &Mutex<Option<Duration>>,
    progress_source: &Mutex<Option<glib::SourceId>>,
    progress_interval: &Mutex<Duration>,
    duration_probes: &Mutex<u32>,
    context: &glib::MainContext,
) {
    let generation = *generation
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match message.view() {
        gst::MessageView::Eos(_) => handler(PlayerEvent::EndOfStream { generation }),
        gst::MessageView::Error(error) => handler(PlayerEvent::Error {
            generation,
            message: format_bus_error(error),
        }),
        gst::MessageView::StateChanged(changed) if is_playbin_message(playbin, message) => {
            let state = match changed.current() {
                gst::State::Playing => PlaybackState::Playing,
                gst::State::Paused => PlaybackState::Paused,
                _ => PlaybackState::Stopped,
            };
            handler(PlayerEvent::StateChanged { generation, state });
            if state == PlaybackState::Playing {
                emit_progress(
                    playbin,
                    handler,
                    generation,
                    known_duration,
                    duration_probes,
                );
                start_progress(
                    progress_source,
                    context,
                    playbin,
                    handler,
                    &Mutex::new(generation),
                    known_duration,
                    progress_interval,
                    duration_probes,
                );
            } else {
                stop_progress(progress_source, context);
            }
        }
        gst::MessageView::DurationChanged(_) if is_playbin_message(playbin, message) => {
            if let Some(duration) = query_clock(playbin, QueryClock::Duration) {
                *known_duration
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(duration);
                emit_progress(
                    playbin,
                    handler,
                    generation,
                    known_duration,
                    duration_probes,
                );
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn start_progress(
    progress_source: &Mutex<Option<glib::SourceId>>,
    context: &glib::MainContext,
    playbin: &gst::Element,
    handler: &Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    generation: &Mutex<LoadGeneration>,
    known_duration: &Mutex<Option<Duration>>,
    progress_interval: &Mutex<Duration>,
    duration_probes: &Mutex<u32>,
) {
    let mut slot = progress_source
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if slot.is_some() {
        return;
    }
    let playbin = playbin.clone();
    let handler = Arc::clone(handler);
    let generation = Arc::new(Mutex::new(
        *generation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    ));
    let known_duration = Arc::new(Mutex::new(
        *known_duration
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    ));
    let duration_probes = Arc::new(Mutex::new(
        *duration_probes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    ));
    let interval = *progress_interval
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let source = glib::timeout_source_new(
        interval,
        Some("qingyin-gst-progress"),
        glib::Priority::DEFAULT,
        move || {
            let generation = *generation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            emit_progress(
                &playbin,
                &handler,
                generation,
                &known_duration,
                &duration_probes,
            );
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

fn emit_progress(
    playbin: &gst::Element,
    handler: &Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    generation: LoadGeneration,
    known_duration: &Mutex<Option<Duration>>,
    duration_probes: &Mutex<u32>,
) {
    let Some(position) = query_clock(playbin, QueryClock::Position) else {
        return;
    };
    let mut duration = *known_duration
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if duration.is_none() {
        let mut probes = duration_probes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *probes < DURATION_PROBE_LIMIT {
            *probes += 1;
            duration = query_clock(playbin, QueryClock::Duration);
            if duration.is_some() {
                *known_duration
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = duration;
            }
        }
    }
    handler(PlayerEvent::Progress {
        generation,
        position,
        duration,
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::RecvTimeoutError;

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
    }

    #[test]
    fn formats_bus_errors_with_optional_debug() {
        assert_eq!(format_error_parts("decode failed", None), "decode failed");
        assert_eq!(
            format_error_parts("decode failed", Some("no decoder")),
            "decode failed: no decoder"
        );
    }

    #[test]
    fn fake_backend_reports_load_failures_and_ignores_stale_generation_in_handle() {
        let (events_tx, events_rx) = mpsc::channel();
        let (mut player, fake) = Player::spawn_fake(move |event| {
            let _ = events_tx.send(event);
        });
        wait_for(&events_rx, |event| matches!(event, PlayerEvent::Ready));
        player.mark_ready();
        fake.set_missing("/missing.flac", true);
        let _ = player.load("/missing.flac");
        let failed = wait_for(&events_rx, |event| {
            matches!(
                event,
                PlayerEvent::CommandFinished {
                    kind: CommandKind::Load,
                    error: Some(_),
                    ..
                }
            )
        });
        assert!(matches!(
            failed,
            PlayerEvent::CommandFinished {
                kind: CommandKind::Load,
                error: Some(_),
                ..
            }
        ));
        let old = player.generation();
        fake.set_missing("/ok.flac", false);
        let _ = player.load("/ok.flac");
        wait_for(&events_rx, |event| {
            matches!(
                event,
                PlayerEvent::CommandFinished {
                    kind: CommandKind::Load,
                    error: None,
                    ..
                }
            )
        });
        fake.inject(PlayerEvent::EndOfStream { generation: old });
        fake.inject(PlayerEvent::Progress {
            generation: old,
            position: Duration::from_secs(9),
            duration: Some(Duration::from_secs(10)),
        });
        fake.inject(PlayerEvent::EndOfStream {
            generation: player.generation(),
        });
        let eos = wait_for(
            &events_rx,
            |event| matches!(event, PlayerEvent::EndOfStream { generation } if *generation == player.generation()),
        );
        match eos {
            PlayerEvent::EndOfStream { generation } => assert_eq!(generation, player.generation()),
            other => panic!("{other:?}"),
        }
        let _ = player.shutdown();
    }

    #[test]
    fn fake_play_pause_and_seek_complete_with_command_ids() {
        let (events_tx, events_rx) = mpsc::channel();
        let (mut player, fake) = Player::spawn_fake(move |event| {
            let _ = events_tx.send(event);
        });
        wait_for(&events_rx, |event| matches!(event, PlayerEvent::Ready));
        fake.set_missing("/ok.flac", false);
        let load_id = player.load("/ok.flac");
        match wait_for(&events_rx, |event| {
            matches!(
                event,
                PlayerEvent::CommandFinished {
                    kind: CommandKind::Load,
                    ..
                }
            )
        }) {
            PlayerEvent::CommandFinished {
                id, error: None, ..
            } => assert_eq!(id, load_id),
            other => panic!("{other:?}"),
        }
        let play_id = player.play();
        match wait_for(&events_rx, |event| {
            matches!(
                event,
                PlayerEvent::CommandFinished {
                    kind: CommandKind::Play,
                    ..
                }
            )
        }) {
            PlayerEvent::CommandFinished {
                id, error: None, ..
            } => assert_eq!(id, play_id),
            other => panic!("{other:?}"),
        }
        fake.fail_next(CommandKind::Seek, "seek failed");
        let seek_id = player.seek(Duration::from_secs(5));
        match wait_for(&events_rx, |event| {
            matches!(
                event,
                PlayerEvent::CommandFinished {
                    kind: CommandKind::Seek,
                    ..
                }
            )
        }) {
            PlayerEvent::CommandFinished {
                id,
                error: Some(error),
                ..
            } => {
                assert_eq!(id, seek_id);
                assert!(error.contains("seek failed"));
            }
            other => panic!("{other:?}"),
        }
        let _ = player.shutdown();
    }

    fn wait_for(
        rx: &Receiver<PlayerEvent>,
        mut predicate: impl FnMut(&PlayerEvent) -> bool,
    ) -> PlayerEvent {
        loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(event) if predicate(&event) => return event,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for player event"),
                Err(RecvTimeoutError::Disconnected) => panic!("player closed"),
            }
        }
    }

    #[test]
    fn gst_player_loads_generated_wav_with_fakesink() {
        gst::init().ok();
        let directory = std::env::temp_dir().join(format!(
            "qingyin-player-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("tone.wav");
        qingyin_wav(&path);
        let (events_tx, events_rx) = mpsc::channel();
        let mut player = Player::spawn(move |event| {
            let _ = events_tx.send(event);
        });
        wait_for(&events_rx, |event| matches!(event, PlayerEvent::Ready));
        player.mark_ready();
        let _ = player.load(&path);
        let loaded = wait_for(&events_rx, |event| {
            matches!(
                event,
                PlayerEvent::CommandFinished {
                    kind: CommandKind::Load,
                    ..
                }
            )
        });
        assert!(matches!(
            loaded,
            PlayerEvent::CommandFinished { error: None, .. }
        ));
        let _ = player.play();
        wait_for(&events_rx, |event| {
            matches!(
                event,
                PlayerEvent::StateChanged {
                    state: PlaybackState::Playing,
                    ..
                } | PlayerEvent::Progress { .. }
                    | PlayerEvent::CommandFinished {
                        kind: CommandKind::Play,
                        error: None,
                        ..
                    }
            )
        });
        let _ = player.pause();
        let _ = player.shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }

    fn qingyin_wav(path: &Path) {
        let data_len = 1600u32;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + data_len).to_le_bytes());
        bytes.extend(b"WAVE");
        bytes.extend(b"fmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(8000u32.to_le_bytes());
        bytes.extend(16000u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(data_len.to_le_bytes());
        bytes.extend(vec![0_u8; data_len as usize]);
        std::fs::write(path, bytes).unwrap();
    }
}
