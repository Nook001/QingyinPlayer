use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use image::{ImageFormat, ImageReader, Limits};
use qingyin_core::XdgDirs;
use qingyin_metadata::{CoverArt, FileFingerprint, TrackMetadata};
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::Url;

pub const CACHE_VERSION: u32 = 2;
pub const MAX_COVER_SOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_COVER_DECODE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_COVER_SOURCE_EDGE: u32 = 8192;
pub const MAX_COVER_EDGE: u32 = 512;
pub const MAX_IN_FLIGHT_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const COVER_WORKERS: usize = 4;
const REQUEST_MAILBOX: usize = 4096;
const WORK_MAILBOX: usize = 4;
const WORKER_JOIN: Duration = Duration::from_millis(100);

#[derive(Debug, Error)]
pub enum CoverError {
    #[error("failed to access cover cache {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("unable to resolve cover cache directory")]
    MissingDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverFailure {
    Missing,
    Oversized,
    DecodeFailed,
    TemporaryIo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverLookup {
    Ready(String),
    Failed(CoverFailure),
    Absent,
}

/// Qt-independent cover cache: digest-addressed files, fingerprint mappings, atomic writes.
#[derive(Debug, Clone)]
pub struct CoverService {
    directory: PathBuf,
}

impl CoverService {
    /// Opens (and creates) the default XDG cover cache.
    ///
    /// # Errors
    ///
    /// Returns [`CoverError`] when the cache directory cannot be created.
    pub fn open_default() -> Result<Self, CoverError> {
        let directory = XdgDirs::resolve()
            .map_err(|_| CoverError::MissingDirectory)?
            .cover_cache_dir();
        Self::open(directory)
    }

    /// Opens (and creates) `directory` as a cover cache root.
    ///
    /// # Errors
    ///
    /// Returns [`CoverError`] when the directory cannot be created.
    pub fn open(directory: impl Into<PathBuf>) -> Result<Self, CoverError> {
        let directory = directory.into();
        fs::create_dir_all(directory.join("files")).map_err(|source| CoverError::Io {
            path: directory.clone(),
            source,
        })?;
        fs::create_dir_all(directory.join("maps")).map_err(|source| CoverError::Io {
            path: directory.clone(),
            source,
        })?;
        Ok(Self { directory })
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    #[must_use]
    pub fn lookup(&self, track: &TrackMetadata) -> CoverLookup {
        self.lookup_key(
            &track.path,
            track.fingerprint(),
            track.cover_digest.as_deref(),
        )
    }

    #[must_use]
    pub fn lookup_key(
        &self,
        path: &Path,
        fingerprint: FileFingerprint,
        digest: Option<&str>,
    ) -> CoverLookup {
        if let Some(digest) = digest
            && let Some(url) = self.url_for_digest(digest)
        {
            return CoverLookup::Ready(url);
        }
        match self.mapping(path, &fingerprint) {
            Some(Mapping::Digest(digest)) => self
                .url_for_digest(&digest)
                .map_or(CoverLookup::Absent, CoverLookup::Ready),
            Some(Mapping::Failed(failure)) => CoverLookup::Failed(failure),
            None => CoverLookup::Absent,
        }
    }

    #[must_use]
    pub fn url_for_digest(&self, digest: &str) -> Option<String> {
        let path = self.file_for_digest(digest);
        path.is_file()
            .then(|| Url::from_file_path(path).ok().map(Into::into))
            .flatten()
    }

    /// Stores `art` under its content digest, replacing any previous mapping for `fingerprint`.
    ///
    /// # Errors
    ///
    /// Returns [`CoverError`] on I/O failures. Oversized or undecodable images are recorded as
    /// permanent failures and return `Ok(None)`.
    pub fn store(
        &self,
        path: &Path,
        fingerprint: FileFingerprint,
        art: &CoverArt,
    ) -> Result<Option<String>, CoverError> {
        if art.data.is_empty() {
            self.remember_failure(path, fingerprint, CoverFailure::Missing)?;
            return Ok(None);
        }
        if art.data.len() > MAX_COVER_SOURCE_BYTES {
            self.remember_failure(path, fingerprint, CoverFailure::Oversized)?;
            return Ok(None);
        }
        let digest = art.digest();
        let file = self.file_for_digest(&digest);
        if !is_valid_cache_file(&file) {
            let Some(bytes) = encode_cached_cover(&art.data) else {
                self.remember_failure(path, fingerprint, CoverFailure::DecodeFailed)?;
                return Ok(None);
            };
            atomic_write(&file, &bytes)?;
        }
        self.remember_digest(path, fingerprint, &digest)?;
        Ok(self.url_for_digest(&digest))
    }

    /// Records that the track has no usable embedded picture.
    ///
    /// # Errors
    ///
    /// Returns [`CoverError`] when the mapping file cannot be written.
    pub fn remember_missing(
        &self,
        path: &Path,
        fingerprint: FileFingerprint,
    ) -> Result<(), CoverError> {
        self.remember_failure(path, fingerprint, CoverFailure::Missing)
    }

    /// Records a temporary I/O failure so the scheduler can retry later.
    ///
    /// # Errors
    ///
    /// Returns [`CoverError`] when the mapping file cannot be written.
    pub fn remember_temporary_io(
        &self,
        path: &Path,
        fingerprint: FileFingerprint,
    ) -> Result<(), CoverError> {
        self.remember_failure(path, fingerprint, CoverFailure::TemporaryIo)
    }

    /// Removes cache files that are not in `protected` until `max_bytes` is respected.
    ///
    /// # Errors
    ///
    /// Returns [`CoverError`] when the cache directory cannot be read.
    pub fn prune(&self, max_bytes: u64, protected: &HashSet<String>) -> Result<u64, CoverError> {
        let files_dir = self.directory.join("files");
        let mut entries = Vec::new();
        let read = fs::read_dir(&files_dir).map_err(|source| CoverError::Io {
            path: files_dir.clone(),
            source,
        })?;
        for entry in read {
            let entry = entry.map_err(|source| CoverError::Io {
                path: files_dir.clone(),
                source,
            })?;
            let path = entry.path();
            let metadata = entry.metadata().map_err(|source| CoverError::Io {
                path: path.clone(),
                source,
            })?;
            let modified = metadata.modified().ok();
            entries.push((path, metadata.len(), modified));
        }
        entries.sort_by_key(|entry| entry.2);
        let mut total: u64 = entries.iter().map(|entry| entry.1).sum();
        for (path, size, _) in entries {
            if total <= max_bytes {
                break;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if protected.contains(stem) {
                continue;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        Ok(total)
    }

    fn file_for_digest(&self, digest: &str) -> PathBuf {
        self.directory
            .join("files")
            .join(format!("v{CACHE_VERSION}-{digest}.png"))
    }

    fn map_path(&self, path: &Path, fingerprint: FileFingerprint) -> PathBuf {
        self.directory.join("maps").join(format!(
            "v{CACHE_VERSION}-{}",
            mapping_key(path, fingerprint)
        ))
    }

    fn mapping(&self, path: &Path, fingerprint: &FileFingerprint) -> Option<Mapping> {
        let text = fs::read_to_string(self.map_path(path, *fingerprint)).ok()?;
        parse_mapping(&text)
    }

    fn remember_digest(
        &self,
        path: &Path,
        fingerprint: FileFingerprint,
        digest: &str,
    ) -> Result<(), CoverError> {
        atomic_write(
            &self.map_path(path, fingerprint),
            format!("digest:{digest}").as_bytes(),
        )
    }

    fn remember_failure(
        &self,
        path: &Path,
        fingerprint: FileFingerprint,
        failure: CoverFailure,
    ) -> Result<(), CoverError> {
        let token = match failure {
            CoverFailure::Missing => "missing",
            CoverFailure::Oversized => "oversized",
            CoverFailure::DecodeFailed => "decode",
            CoverFailure::TemporaryIo => "tempio",
        };
        atomic_write(
            &self.map_path(path, fingerprint),
            format!("fail:{token}").as_bytes(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mapping {
    Digest(String),
    Failed(CoverFailure),
}

fn parse_mapping(text: &str) -> Option<Mapping> {
    let text = text.trim();
    if let Some(digest) = text.strip_prefix("digest:") {
        return Some(Mapping::Digest(digest.to_owned()));
    }
    let token = text.strip_prefix("fail:")?;
    let failure = match token {
        "missing" => CoverFailure::Missing,
        "oversized" => CoverFailure::Oversized,
        "decode" => CoverFailure::DecodeFailed,
        "tempio" => CoverFailure::TemporaryIo,
        _ => return None,
    };
    Some(Mapping::Failed(failure))
}

fn mapping_key(path: &Path, fingerprint: FileFingerprint) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_os_str().as_encoded_bytes());
    hasher.update(fingerprint.modified_at_ns.to_le_bytes());
    hasher.update(fingerprint.file_size.to_le_bytes());
    hex_bytes(hasher.finalize().as_slice())
}

fn is_valid_cache_file(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 8];
    let Ok(read) = file.read(&mut header) else {
        return false;
    };
    read >= 3 && (looks_like_png(&header) || looks_like_jpeg(&header))
}

fn looks_like_png(header: &[u8]) -> bool {
    header.starts_with(&[0x89, b'P', b'N', b'G'])
}

fn looks_like_jpeg(header: &[u8]) -> bool {
    header.starts_with(&[0xFF, 0xD8, 0xFF])
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn encode_cached_cover(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() > MAX_COVER_SOURCE_BYTES {
        return None;
    }
    if (looks_like_png(data) || looks_like_jpeg(data))
        && let Some((width, height)) = image_dimensions(data)
        && width > 0
        && height > 0
        && width <= MAX_COVER_EDGE
        && height <= MAX_COVER_EDGE
    {
        return Some(data.to_vec());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_COVER_SOURCE_EDGE);
    limits.max_image_height = Some(MAX_COVER_SOURCE_EDGE);
    limits.max_alloc = Some(MAX_COVER_DECODE_BYTES);
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits);
    let image = reader
        .decode()
        .ok()?
        .thumbnail(MAX_COVER_EDGE, MAX_COVER_EDGE);
    let mut output = Cursor::new(Vec::new());
    image
        .into_rgb8()
        .write_to(&mut output, ImageFormat::Jpeg)
        .ok()?;
    Some(output.into_inner())
}

fn image_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CoverError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| CoverError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let temp_path = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .map_err(|source| CoverError::Io {
                path: temp_path.clone(),
                source,
            })?;
        file.write_all(bytes).map_err(|source| CoverError::Io {
            path: temp_path.clone(),
            source,
        })?;
        drop(file);
        fs::rename(&temp_path, path).map_err(|source| CoverError::Io {
            path: path.to_path_buf(),
            source,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoverPriority {
    Playing = 0,
    Visible = 1,
    Background = 2,
}

#[derive(Debug, Clone)]
pub struct CoverRequest {
    pub track_id: i64,
    pub path: PathBuf,
    pub fingerprint: FileFingerprint,
    pub cover_digest: Option<String>,
    pub priority: CoverPriority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverUpdate {
    pub track_id: i64,
    pub url: String,
}

pub struct CoverScheduler {
    tx: SyncSender<SchedulerMessage>,
    worker: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl std::fmt::Debug for CoverScheduler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CoverScheduler")
            .finish_non_exhaustive()
    }
}

enum SchedulerMessage {
    Request(Box<CoverRequest>),
    Protect(Vec<String>),
    Prune,
    Stop,
}

impl CoverScheduler {
    #[must_use]
    pub fn start(
        service: Arc<CoverService>,
        on_update: impl Fn(CoverUpdate) + Send + Sync + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::sync_channel(REQUEST_MAILBOX);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_service = Arc::clone(&service);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("qingyin-covers".into())
            .spawn(move || run_scheduler(worker_service, rx, worker_stop, on_update))
            .ok();
        Self { tx, worker, stop }
    }

    pub fn request(&self, request: CoverRequest) {
        match self
            .tx
            .try_send(SchedulerMessage::Request(Box::new(request)))
        {
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
            Err(TrySendError::Full(SchedulerMessage::Request(request))) => {
                let _ = self.tx.try_send(SchedulerMessage::Request(request));
            }
            Err(TrySendError::Full(_)) => {}
        }
    }

    pub fn protect(&self, digests: Vec<String>) {
        let _ = self.tx.try_send(SchedulerMessage::Protect(digests));
    }

    pub fn request_prune(&self) {
        let _ = self.tx.try_send(SchedulerMessage::Prune);
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.tx.try_send(SchedulerMessage::Stop);
        if let Some(worker) = self.worker.take() {
            let (tx, rx) = mpsc::sync_channel(1);
            thread::spawn(move || {
                let _ = worker.join();
                let _ = tx.send(());
            });
            let _ = rx.recv_timeout(WORKER_JOIN);
        }
    }
}

impl Drop for CoverScheduler {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_scheduler(
    service: Arc<CoverService>,
    rx: Receiver<SchedulerMessage>,
    stop: Arc<AtomicBool>,
    on_update: impl Fn(CoverUpdate) + Send + Sync + 'static,
) {
    let on_update = Arc::new(on_update);
    let (work_tx, work_rx) = mpsc::sync_channel::<CoverRequest>(WORK_MAILBOX);
    let work_rx = Arc::new(Mutex::new(work_rx));
    let inflight_bytes = Arc::new(AtomicUsize::new(0));
    for index in 0..COVER_WORKERS {
        let work_rx = Arc::clone(&work_rx);
        let service = Arc::clone(&service);
        let stop = Arc::clone(&stop);
        let on_update = Arc::clone(&on_update);
        let inflight_bytes = Arc::clone(&inflight_bytes);
        let _ = thread::Builder::new()
            .name(format!("qingyin-cover-{index}"))
            .spawn(move || cover_worker(service, work_rx, stop, inflight_bytes, on_update));
    }
    let mut queued: HashMap<i64, CoverRequest> = HashMap::new();
    let mut protected = HashSet::new();
    let mut pruned = false;
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let message = if queued.is_empty() {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            rx.try_recv().ok()
        };
        match message {
            Some(SchedulerMessage::Stop) | None if stop.load(Ordering::Relaxed) => break,
            Some(SchedulerMessage::Stop) => break,
            Some(SchedulerMessage::Protect(digests)) => {
                protected = digests.into_iter().collect();
            }
            Some(SchedulerMessage::Prune) => {
                let _ = service.prune(DEFAULT_CACHE_BYTES, &protected);
                pruned = true;
            }
            Some(SchedulerMessage::Request(request)) => {
                let request = *request;
                queued
                    .entry(request.track_id)
                    .and_modify(|existing| {
                        if request.priority < existing.priority {
                            *existing = request.clone();
                        }
                    })
                    .or_insert(request);
            }
            None => {
                if !pruned && queued.is_empty() {
                    pruned = true;
                    let _ = service.prune(DEFAULT_CACHE_BYTES, &protected);
                }
            }
        }
        while !stop.load(Ordering::Relaxed) {
            let Some(id) = next_request_id(&queued) else {
                break;
            };
            let Some(request) = queued.remove(&id) else {
                break;
            };
            match work_tx.try_send(request) {
                Ok(()) => {}
                Err(TrySendError::Full(request)) => {
                    queued.insert(request.track_id, request);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => return,
            }
        }
    }
}

fn cover_worker(
    service: Arc<CoverService>,
    work_rx: Arc<Mutex<Receiver<CoverRequest>>>,
    stop: Arc<AtomicBool>,
    inflight_bytes: Arc<AtomicUsize>,
    on_update: Arc<dyn Fn(CoverUpdate) + Send + Sync>,
) {
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let request = {
            let receiver = work_rx
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(request) => request,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        };
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(url) = fulfill(&service, &request, &inflight_bytes) {
            on_update(CoverUpdate {
                track_id: request.track_id,
                url,
            });
        }
    }
}

fn next_request_id(queued: &HashMap<i64, CoverRequest>) -> Option<i64> {
    queued
        .values()
        .min_by_key(|request| request.priority)
        .map(|request| request.track_id)
}

fn fulfill(
    service: &CoverService,
    request: &CoverRequest,
    inflight_bytes: &AtomicUsize,
) -> Option<String> {
    let _span = tracing::info_span!("covers.fulfill", track_id = request.track_id).entered();
    match service.lookup_key(
        &request.path,
        request.fingerprint,
        request.cover_digest.as_deref(),
    ) {
        CoverLookup::Ready(url) => return Some(url),
        CoverLookup::Failed(CoverFailure::TemporaryIo) | CoverLookup::Absent => {}
        CoverLookup::Failed(_) => return None,
    }
    let art = match qingyin_metadata::read_cover(&request.path) {
        Ok(Some(art)) => art,
        Ok(None) => {
            let _ = service.remember_missing(&request.path, request.fingerprint);
            return None;
        }
        Err(_) => {
            let _ = service.remember_temporary_io(&request.path, request.fingerprint);
            return None;
        }
    };
    while inflight_bytes
        .load(Ordering::Relaxed)
        .saturating_add(art.data.len())
        > MAX_IN_FLIGHT_BYTES
    {
        thread::sleep(Duration::from_millis(5));
    }
    inflight_bytes.fetch_add(art.data.len(), Ordering::Relaxed);
    let stored = service.store(&request.path, request.fingerprint, &art);
    inflight_bytes.fetch_sub(art.data.len(), Ordering::Relaxed);
    stored.ok().flatten()
}

/// Applies a parsed cover immediately, without a second Lofty read.
pub fn commit_parsed_cover(
    service: &CoverService,
    track: &mut TrackMetadata,
    cover: Option<CoverArt>,
) -> Option<String> {
    let fingerprint = track.fingerprint();
    match cover {
        Some(art) => {
            track.cover_digest = Some(art.digest());
            service.store(&track.path, fingerprint, &art).ok().flatten()
        }
        None => {
            let _ = service.remember_missing(&track.path, fingerprint);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reuses_digest_addressed_files() {
        let directory = unique_dir("covers");
        let service = CoverService::open(&directory).unwrap();
        let png = tiny_png();
        let art = CoverArt {
            data: png.clone(),
            extension: "png".into(),
        };
        let fingerprint = FileFingerprint {
            modified_at_ns: 1,
            file_size: 10,
        };
        let path = Path::new("/music/a.flac");
        let url = service
            .store(path, fingerprint, &art)
            .unwrap()
            .expect("url");
        assert!(url.starts_with("file:"));
        let cached = fs::read(service.file_for_digest(&art.digest())).unwrap();
        assert_eq!(cached, png);
        let again = service
            .store(path, fingerprint, &art)
            .unwrap()
            .expect("url");
        assert_eq!(url, again);
        let files = fs::read_dir(directory.join("files")).unwrap().count();
        assert_eq!(files, 1);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_oversized_sources_before_decoding() {
        let directory = unique_dir("covers-oversize");
        let service = CoverService::open(&directory).unwrap();
        let art = CoverArt {
            data: vec![0; MAX_COVER_SOURCE_BYTES + 1],
            extension: "png".into(),
        };
        let fingerprint = FileFingerprint {
            modified_at_ns: 2,
            file_size: 11,
        };
        let path = Path::new("/music/a.flac");
        assert!(service.store(path, fingerprint, &art).unwrap().is_none());
        let mut track = TrackMetadata::from_display("/music/a.flac", "a", None, Vec::new(), None);
        track.file_size = 11;
        track.modified_at_ns = 2;
        assert!(matches!(
            service.lookup(&track),
            CoverLookup::Failed(CoverFailure::Oversized)
        ));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn corrupted_cache_files_are_replaced() {
        let directory = unique_dir("covers-corrupt");
        let service = CoverService::open(&directory).unwrap();
        let art = CoverArt {
            data: tiny_png(),
            extension: "png".into(),
        };
        let digest = art.digest();
        let path = service.file_for_digest(&digest);
        fs::write(&path, b"not-a-png").unwrap();
        let fingerprint = FileFingerprint {
            modified_at_ns: 3,
            file_size: 12,
        };
        let url = service
            .store(Path::new("/music/a.flac"), fingerprint, &art)
            .unwrap()
            .expect("url");
        assert!(Url::parse(&url).is_ok());
        let bytes = fs::read(path).unwrap();
        assert_ne!(bytes, b"not-a-png");
        let _ = fs::remove_dir_all(directory);
    }

    fn unique_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "qingyin-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn tiny_png() -> Vec<u8> {
        let image = image::DynamicImage::new_rgb8(8, 8);
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }
}
