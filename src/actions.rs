//! External action handlers for interacting with the host OS.
//!
//! Manages detached process spawning for browsers and media players, as well
//! as multi-backend clipboard copy operations.

use std::process::{Command, Stdio};

/// Represents the status outcome of executing an external action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOutcome {
    /// Browser successfully opened the URL in a detached process.
    BrowserOpened(String),

    /// URL copied to system clipboard.
    CopiedToClipboard(String),

    /// Media player launched in background.
    PlayerLaunched(String),

    /// A background file transfer has started.
    DownloadStarted(PathBuf),

    /// A matching transfer is already managed by the application.
    DownloadAlreadyQueued(String),

    /// A running transfer has been asked to stop.
    DownloadCancelRequested(String),

    /// Action failed with an error description.
    Error(String),
}

impl ActionOutcome {
    /// User-facing status line notification.
    pub fn message(&self) -> String {
        match self {
            ActionOutcome::BrowserOpened(url) => format!("🌐 Opened in browser: {}", url),
            ActionOutcome::CopiedToClipboard(url) => {
                format!("📋 Copied link to clipboard: {}", url)
            }
            ActionOutcome::PlayerLaunched(player) => {
                format!("🎬 Launched {} player in background", player)
            }
            ActionOutcome::DownloadStarted(path) => {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("file");
                format!("⬇ Download started: {name}")
            }
            ActionOutcome::DownloadAlreadyQueued(filename) => {
                format!("⬇ Already downloading: {filename}")
            }
            ActionOutcome::DownloadCancelRequested(filename) => {
                format!("⬇ Cancelling download: {filename}")
            }
            ActionOutcome::Error(err) => format!("⚠️ {}", err),
        }
    }

    /// Checks if this outcome represents a failure.
    pub fn is_error(&self) -> bool {
        matches!(self, ActionOutcome::Error(_))
    }
}

pub type ActionResult = anyhow::Result<ActionOutcome>;

/// Opens the specified URL in the host's default web browser.
///
/// Spawns as a detached process with `null` standard streams so the TUI
/// terminal loop remains responsive and never blocks.
pub fn open_in_browser(url: &str) -> ActionOutcome {
    match open::that_detached(url) {
        Ok(()) => ActionOutcome::BrowserOpened(url.to_string()),
        Err(e) => {
            // Fallback to xdg-open on Linux desktop environments
            let status = Command::new("xdg-open")
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();

            match status {
                Ok(_) => ActionOutcome::BrowserOpened(url.to_string()),
                Err(_) => ActionOutcome::Error(format!("Failed to open browser: {}", e)),
            }
        }
    }
}

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

/// State updates emitted by the background file downloader.
#[derive(Debug)]
pub enum DownloadEvent {
    /// The transfer made progress; `total` is absent when the server does not send a size.
    Progress {
        /// Application-owned download identifier.
        id: u64,
        /// Bytes written to the temporary file.
        downloaded: u64,
        /// Final expected byte count, when known.
        total: Option<u64>,
    },
    /// The temporary file was promoted to its final destination.
    Completed {
        /// Application-owned download identifier.
        id: u64,
        /// Completed file path.
        destination: PathBuf,
        /// Final byte count.
        downloaded: u64,
    },
    /// The transfer could not be completed.
    Failed {
        /// Application-owned download identifier.
        id: u64,
        /// Human-readable cause.
        error: String,
    },
    /// The transfer stopped at the user's request; its partial file remains for resume.
    Cancelled {
        /// Application-owned download identifier.
        id: u64,
        /// Number of bytes retained in the temporary partial file.
        downloaded: u64,
    },
}

/// Returns the conventional per-user download directory for the current platform.
///
/// A caller can always override this with the `--downloads-dir` command-line option.
pub fn default_download_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_DOWNLOAD_DIR") {
        return PathBuf::from(path);
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(home).join("Downloads");
        }
    }

    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Downloads"))
        .unwrap_or_else(|| PathBuf::from("Downloads"))
}

/// Parses the human-readable size stored in the media index into bytes.
///
/// The crawler uses binary-style units (`MiB`/`GiB`) and occasionally emits
/// decimal-style aliases (`MB`/`GB`); both are treated as powers of 1024 so the
/// progress bar remains consistent with the byte formatter in the TUI.
pub fn parse_size_hint(value: Option<&str>) -> Option<u64> {
    let value = value?.trim();
    let mut parts = value.split_whitespace();
    let amount = parts.next()?.parse::<f64>().ok()?;
    if !amount.is_finite() || amount < 0.0 {
        return None;
    }
    let unit = parts.next().unwrap_or("B").to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "b" => 1_u64,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024_u64.pow(2),
        "g" | "gb" | "gib" => 1024_u64.pow(3),
        "t" | "tb" | "tib" => 1024_u64.pow(4),
        _ => return None,
    };
    Some((amount * multiplier as f64).round() as u64)
}

/// Starts a native resumable file transfer on a detached worker thread.
///
/// Servers that support HTTP byte ranges use up to eight parallel Rust worker
/// threads. Servers without range support use the single-connection fallback.
/// Both modes write to `<name>.part` first and only rename after completion.
pub fn start_download(
    id: u64,
    url: String,
    filename: String,
    directory: PathBuf,
    updates: Sender<DownloadEvent>,
    cancellation: Arc<AtomicBool>,
    total_hint: Option<u64>,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&directory)?;
    let destination = available_destination(&directory, &filename);
    let part_path = part_path_for(&destination);
    let worker_destination = destination.clone();

    thread::Builder::new()
        .name(format!("download-{id}"))
        .spawn(move || {
            let context = DownloadContext {
                updates: &updates,
                cancellation: Arc::clone(&cancellation),
                total_hint,
            };
            let result = match download_with_parallel_ranges(
                id,
                &url,
                &part_path,
                &worker_destination,
                &context,
            ) {
                Ok(ParallelDownloadResult::Completed) => Ok(None),
                Ok(ParallelDownloadResult::Cancelled(downloaded)) => Ok(Some(downloaded)),
                Ok(ParallelDownloadResult::Unsupported) => download_to_path(
                    id,
                    &url,
                    &part_path,
                    &worker_destination,
                    &updates,
                    &context.cancellation,
                    context.total_hint,
                ),
                Err(error) => Err(error),
            };
            match result {
                Ok(Some(downloaded)) => {
                    let _ = updates.send(DownloadEvent::Cancelled { id, downloaded });
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = updates.send(DownloadEvent::Failed {
                        id,
                        error: error.to_string(),
                    });
                }
            }
        })?;

    Ok(destination)
}

fn available_destination(directory: &Path, filename: &str) -> PathBuf {
    let filename = Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("sam-fuzzy-download");
    let base = Path::new(filename);
    let stem = base
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    let extension = base.extension().and_then(|value| value.to_str());

    let candidate = directory.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    for index in 1.. {
        let name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = directory.join(name);
        if !candidate.exists() && !part_path_for(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!("unbounded counter always yields a path")
}

fn part_path_for(destination: &Path) -> PathBuf {
    let mut value = destination.as_os_str().to_owned();
    value.push(".part");
    PathBuf::from(value)
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn parallel_state_path(part_path: &Path) -> PathBuf {
    let mut value = part_path.as_os_str().to_owned();
    value.push(".sam-fuzzy.json");
    PathBuf::from(value)
}

struct DownloadContext<'a> {
    updates: &'a Sender<DownloadEvent>,
    cancellation: Arc<AtomicBool>,
    total_hint: Option<u64>,
}

const PARALLEL_CONNECTIONS: usize = 8;
const RANGE_CHUNK_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    fn len(&self) -> u64 {
        self.end.saturating_sub(self.start).saturating_add(1)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ParallelState {
    url: String,
    total: u64,
    completed: Vec<ByteRange>,
}

enum ParallelDownloadResult {
    Completed,
    Cancelled(u64),
    Unsupported,
}

enum RangeEvent {
    Completed(ByteRange),
    Failed(String),
}

struct RangeWorkerContext {
    client: reqwest::blocking::Client,
    url: String,
    total: u64,
    output: Arc<Mutex<File>>,
    cancellation: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    downloaded: Arc<AtomicU64>,
}

/// Downloads supported HTTP byte ranges concurrently without requiring an
/// external executable. The state file records finished ranges so a cancelled
/// transfer can continue without trusting sparse-file length alone.
fn download_with_parallel_ranges(
    id: u64,
    url: &str,
    part_path: &Path,
    destination: &Path,
    context: &DownloadContext<'_>,
) -> anyhow::Result<ParallelDownloadResult> {
    if context.cancellation.load(Ordering::Relaxed) {
        return Ok(ParallelDownloadResult::Cancelled(file_size(part_path)));
    }

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()?;
    let Some(total) = probe_range_total(&client, url)? else {
        return Ok(ParallelDownloadResult::Unsupported);
    };
    if total == 0 {
        return Ok(ParallelDownloadResult::Unsupported);
    }

    let state_path = parallel_state_path(part_path);
    let mut state = load_parallel_state(&state_path, part_path, url, total)?;
    state.completed = normalize_ranges(state.completed, total);
    persist_parallel_state(&state_path, &state)?;

    let completed_bytes = ranges_len(&state.completed);
    let pending = split_missing_ranges(total, &state.completed);
    if pending.is_empty() {
        fs::rename(part_path, destination)?;
        let _ = fs::remove_file(state_path);
        let _ = context.updates.send(DownloadEvent::Completed {
            id,
            destination: destination.to_path_buf(),
            downloaded: total,
        });
        return Ok(ParallelDownloadResult::Completed);
    }

    let output = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(false)
        .open(part_path)?;
    output.set_len(total)?;
    let output = Arc::new(Mutex::new(output));
    let pending_count = pending.len();
    let tasks = Arc::new(Mutex::new(VecDeque::from(pending)));
    let stop = Arc::new(AtomicBool::new(false));
    let downloaded = Arc::new(AtomicU64::new(completed_bytes));
    let (event_tx, event_rx) = channel();
    let worker_count = PARALLEL_CONNECTIONS.min(pending_count);
    let mut workers = Vec::with_capacity(worker_count);
    for worker_index in 0..worker_count {
        let worker_context = RangeWorkerContext {
            client: client.clone(),
            url: url.to_string(),
            total,
            output: Arc::clone(&output),
            cancellation: Arc::clone(&context.cancellation),
            stop: Arc::clone(&stop),
            downloaded: Arc::clone(&downloaded),
        };
        let worker_tasks = Arc::clone(&tasks);
        let worker_events = event_tx.clone();
        workers.push(
            thread::Builder::new()
                .name(format!("range-{id}-{worker_index}"))
                .spawn(move || range_worker(worker_tasks, worker_events, worker_context))?,
        );
    }
    drop(event_tx);

    let mut last_report = Instant::now() - Duration::from_secs(1);
    let _ = context.updates.send(DownloadEvent::Progress {
        id,
        downloaded: completed_bytes,
        total: Some(total),
    });

    let mut finished = 0_usize;
    let mut failure = None;
    while finished < pending_count
        && failure.is_none()
        && !context.cancellation.load(Ordering::Relaxed)
    {
        match event_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(RangeEvent::Completed(range)) => {
                state.completed.push(range);
                state.completed = normalize_ranges(state.completed, total);
                persist_parallel_state(&state_path, &state)?;
                finished = finished.saturating_add(1);
            }
            Ok(RangeEvent::Failed(error)) => {
                failure = Some(error);
                stop.store(true, Ordering::Relaxed);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if last_report.elapsed() >= Duration::from_millis(150) {
            let _ = context.updates.send(DownloadEvent::Progress {
                id,
                downloaded: downloaded.load(Ordering::Relaxed),
                total: Some(total),
            });
            last_report = Instant::now();
        }
    }
    stop.store(true, Ordering::Relaxed);
    for worker in workers {
        let _ = worker.join();
    }
    drop(output);

    let current_downloaded = downloaded.load(Ordering::Relaxed);
    let _ = context.updates.send(DownloadEvent::Progress {
        id,
        downloaded: current_downloaded,
        total: Some(total),
    });
    if let Some(error) = failure {
        return Err(anyhow::anyhow!(error));
    }
    if context.cancellation.load(Ordering::Relaxed) || finished < pending_count {
        return Ok(ParallelDownloadResult::Cancelled(current_downloaded));
    }

    fs::rename(part_path, destination)?;
    let _ = fs::remove_file(state_path);
    let _ = context.updates.send(DownloadEvent::Completed {
        id,
        destination: destination.to_path_buf(),
        downloaded: total,
    });
    Ok(ParallelDownloadResult::Completed)
}

fn probe_range_total(client: &reqwest::blocking::Client, url: &str) -> anyhow::Result<Option<u64>> {
    let response = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()?
        .error_for_status()?;
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Ok(None);
    }
    let total = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range)
        .map(|(_, _, total)| total);
    Ok(total)
}

fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?, total.parse().ok()?))
}

fn load_parallel_state(
    state_path: &Path,
    part_path: &Path,
    url: &str,
    total: u64,
) -> anyhow::Result<ParallelState> {
    match fs::read(state_path) {
        Ok(bytes) => match serde_json::from_slice::<ParallelState>(&bytes) {
            Ok(state)
                if state.url == url
                    && state.total == total
                    && fs::metadata(part_path)
                        .map(|metadata| metadata.len() == total)
                        .unwrap_or(false) =>
            {
                Ok(state)
            }
            Ok(_) | Err(_) => Ok(ParallelState {
                url: url.to_string(),
                total,
                completed: Vec::new(),
            }),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let prefix = fs::metadata(part_path)
                .map(|metadata| metadata.len().min(total))
                .unwrap_or(0);
            Ok(ParallelState {
                url: url.to_string(),
                total,
                completed: (prefix > 0)
                    .then_some(ByteRange {
                        start: 0,
                        end: prefix.saturating_sub(1),
                    })
                    .into_iter()
                    .collect(),
            })
        }
        Err(error) => Err(error.into()),
    }
}

fn persist_parallel_state(path: &Path, state: &ParallelState) -> anyhow::Result<()> {
    let encoded = serde_json::to_vec(state)?;
    fs::write(path, encoded)?;
    Ok(())
}

fn normalize_ranges(mut ranges: Vec<ByteRange>, total: u64) -> Vec<ByteRange> {
    ranges.retain(|range| range.start <= range.end && range.start < total);
    for range in &mut ranges {
        range.end = range.end.min(total.saturating_sub(1));
    }
    ranges.sort_by_key(|range| range.start);
    let mut merged = Vec::<ByteRange>::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end.saturating_add(1)
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

fn ranges_len(ranges: &[ByteRange]) -> u64 {
    ranges.iter().map(ByteRange::len).sum()
}

fn split_missing_ranges(total: u64, completed: &[ByteRange]) -> Vec<ByteRange> {
    let mut missing = Vec::new();
    let mut cursor = 0_u64;
    for range in completed {
        if cursor < range.start {
            push_chunks(&mut missing, cursor, range.start.saturating_sub(1));
        }
        cursor = range.end.saturating_add(1);
    }
    if cursor < total {
        push_chunks(&mut missing, cursor, total.saturating_sub(1));
    }
    missing
}

fn push_chunks(output: &mut Vec<ByteRange>, start: u64, end: u64) {
    let mut start = start;
    while start <= end {
        let chunk_end = start
            .saturating_add(RANGE_CHUNK_BYTES.saturating_sub(1))
            .min(end);
        output.push(ByteRange {
            start,
            end: chunk_end,
        });
        if chunk_end == u64::MAX {
            break;
        }
        start = chunk_end + 1;
    }
}

fn range_worker(
    tasks: Arc<Mutex<VecDeque<ByteRange>>>,
    events: Sender<RangeEvent>,
    context: RangeWorkerContext,
) {
    while !context.cancellation.load(Ordering::Relaxed) && !context.stop.load(Ordering::Relaxed) {
        let task = match tasks.lock() {
            Ok(mut tasks) => tasks.pop_front(),
            Err(_) => {
                let _ = events.send(RangeEvent::Failed(
                    "download work queue was poisoned".into(),
                ));
                return;
            }
        };
        let Some(range) = task else {
            return;
        };
        match download_range(&context, &range) {
            Ok(true) => {
                if events.send(RangeEvent::Completed(range)).is_err() {
                    return;
                }
            }
            Ok(false) => return,
            Err(error) => {
                context.stop.store(true, Ordering::Relaxed);
                let _ = events.send(RangeEvent::Failed(error.to_string()));
                return;
            }
        }
    }
}

fn download_range(context: &RangeWorkerContext, range: &ByteRange) -> anyhow::Result<bool> {
    let mut response = context
        .client
        .get(&context.url)
        .header(
            reqwest::header::RANGE,
            format!("bytes={}-{}", range.start, range.end),
        )
        .send()?
        .error_for_status()?;
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        anyhow::bail!("server stopped honoring HTTP range requests");
    }
    let content_range = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range);
    if !matches!(content_range, Some((start, end, total)) if start == range.start && end == range.end && total == context.total)
    {
        anyhow::bail!("server returned a different byte range than requested");
    }

    let mut offset = range.start;
    let mut buffer = [0_u8; 128 * 1024];
    while offset <= range.end {
        if context.cancellation.load(Ordering::Relaxed) || context.stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let read = response.read(&mut buffer)?;
        if read == 0 {
            anyhow::bail!("range response ended before all requested bytes arrived");
        }
        let remaining = range.end.saturating_sub(offset).saturating_add(1) as usize;
        if read > remaining {
            anyhow::bail!("range response exceeded its requested byte span");
        }
        let mut output = context
            .output
            .lock()
            .map_err(|_| anyhow::anyhow!("download output file lock was poisoned"))?;
        output.seek(SeekFrom::Start(offset))?;
        output.write_all(&buffer[..read])?;
        offset = offset.saturating_add(read as u64);
        context.downloaded.fetch_add(read as u64, Ordering::Relaxed);
    }
    Ok(true)
}

fn download_to_path(
    id: u64,
    url: &str,
    part_path: &Path,
    destination: &Path,
    updates: &Sender<DownloadEvent>,
    cancellation: &AtomicBool,
    total_hint: Option<u64>,
) -> anyhow::Result<Option<u64>> {
    if cancellation.load(Ordering::Relaxed) {
        return Ok(Some(
            fs::metadata(part_path).map(|meta| meta.len()).unwrap_or(0),
        ));
    }
    let existing_bytes = fs::metadata(part_path).map(|meta| meta.len()).unwrap_or(0);
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()?;

    let mut request = client.get(url);
    if existing_bytes > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing_bytes}-"));
    }
    let mut response = request.send()?.error_for_status()?;
    if cancellation.load(Ordering::Relaxed) {
        return Ok(Some(existing_bytes));
    }
    let resumed = existing_bytes > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let starting_bytes = if resumed { existing_bytes } else { 0 };
    let total = response
        .content_length()
        .map(|remaining| remaining.saturating_add(starting_bytes))
        .or(total_hint);

    let mut output = if resumed {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(part_path)?
    } else {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(part_path)?
    };

    let mut downloaded = starting_bytes;
    let mut last_report = Instant::now() - Duration::from_secs(1);
    let _ = updates.send(DownloadEvent::Progress {
        id,
        downloaded,
        total,
    });
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        if cancellation.load(Ordering::Relaxed) {
            output.flush()?;
            return Ok(Some(downloaded));
        }
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        downloaded = downloaded.saturating_add(read as u64);
        if last_report.elapsed() >= Duration::from_millis(150) {
            let _ = updates.send(DownloadEvent::Progress {
                id,
                downloaded,
                total,
            });
            last_report = Instant::now();
        }
    }
    output.flush()?;
    drop(output);

    let _ = updates.send(DownloadEvent::Progress {
        id,
        downloaded,
        total,
    });
    if cancellation.load(Ordering::Relaxed) {
        return Ok(Some(downloaded));
    }
    fs::rename(part_path, destination)?;
    let _ = updates.send(DownloadEvent::Completed {
        id,
        destination: destination.to_path_buf(),
        downloaded,
    });
    Ok(None)
}

/// Keeps the in-process clipboard handle alive across invocations.
/// On Linux (X11/Wayland), dropping `Clipboard` immediately terminates selection ownership.
static PERSISTENT_CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// Base64 encoding helper for OSC 52 terminal clipboard escape sequences.
fn base64_encode(bytes: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        out.push(CHARSET[(b0 >> 2) as usize] as char);
        out.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARSET[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Spawns a CLI tool, pipes `text` to its stdin, and returns true if it exited cleanly.
fn copy_via_stdin_cmd(cmd: &str, args: &[&str], text: &str) -> bool {
    let mut command = Command::new(cmd);
    command.args(args);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
        let _ = stdin.flush();
    }

    match child.wait() {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

/// Copies to Wayland compositor using `wl-copy`.
/// Resolves via `PATH`, user home `~/.local/bin`, and system paths.
fn copy_via_wl_copy(text: &str) -> bool {
    if copy_via_stdin_cmd("wl-copy", &[], text) {
        return true;
    }
    if let Ok(home) = std::env::var("HOME") {
        let user_bin = format!("{}/.local/bin/wl-copy", home);
        if copy_via_stdin_cmd(&user_bin, &[], text) {
            return true;
        }
    }
    copy_via_stdin_cmd("/usr/bin/wl-copy", &[], text)
        || copy_via_stdin_cmd("/usr/local/bin/wl-copy", &[], text)
}

/// Copies to X11 selection clipboard using `xclip`.
fn copy_via_xclip(text: &str) -> bool {
    copy_via_stdin_cmd("xclip", &["-selection", "clipboard"], text)
        || copy_via_stdin_cmd("/usr/bin/xclip", &["-selection", "clipboard"], text)
}

/// Copies to X11 selection clipboard using `xsel`.
fn copy_via_xsel(text: &str) -> bool {
    copy_via_stdin_cmd("xsel", &["--clipboard", "--input"], text)
        || copy_via_stdin_cmd("/usr/bin/xsel", &["--clipboard", "--input"], text)
}

/// Emits ANSI OSC 52 sequence directly to terminal stdout.
/// Supported natively by modern terminal emulators (Kitty, Alacritty, WezTerm, Foot, Ghostty).
fn copy_via_osc52(text: &str) -> bool {
    let b64 = base64_encode(text.as_bytes());
    let osc52 = format!("\x1b]52;c;{}\x07", b64);
    if std::io::stdout().write_all(osc52.as_bytes()).is_ok() {
        let _ = std::io::stdout().flush();
        true
    } else {
        false
    }
}

/// In-process clipboard fallback using `arboard`, maintained in a static handle
/// to prevent losing the selection immediately upon function exit.
fn copy_via_arboard(text: &str) -> bool {
    if let Ok(mut guard) = PERSISTENT_CLIPBOARD.lock() {
        if guard.is_none() {
            *guard = arboard::Clipboard::new().ok();
        }
        if let Some(cb) = guard.as_mut() {
            return cb.set_text(text.to_string()).is_ok();
        }
    }
    false
}

/// Copies the given text to the system clipboard using a multi-tiered Linux strategy:
/// 1. `wl-copy` (daemonizes under Wayland to maintain clipboard contents).
/// 2. `xclip` / `xsel` (under X11).
/// 3. In-process persistent `arboard` (kept in static memory).
/// 4. ANSI OSC 52 terminal escape sequence (works over SSH and modern terminals).
pub fn copy_to_clipboard(text: &str) -> ActionOutcome {
    let mut copied = false;

    // 1. Detect Wayland session and prioritize native wl-copy
    let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|s| s.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false);

    if is_wayland && copy_via_wl_copy(text) {
        copied = true;
    }

    // 2. Try X11 native tools if Wayland did not handle it
    if !copied && (copy_via_xclip(text) || copy_via_xsel(text)) {
        copied = true;
    }

    // 3. Fallback: try wl-copy even if WAYLAND_DISPLAY wasn't explicitly set
    if !copied && copy_via_wl_copy(text) {
        copied = true;
    }

    // 4. In-process persistent arboard fallback
    if !copied && copy_via_arboard(text) {
        copied = true;
    }

    // 5. Always emit OSC 52 escape sequence for terminal emulator integration
    let osc52_sent = copy_via_osc52(text);

    if copied || osc52_sent {
        ActionOutcome::CopiedToClipboard(text.to_string())
    } else {
        ActionOutcome::Error(
            "Could not access system clipboard (tried wl-copy, xclip, xsel, arboard)".to_string(),
        )
    }
}

/// Launches a local desktop media player to stream the video directly over HTTP.
///
/// Probes for commonly installed Linux media players (`mpv`, `vlc`, `celluloid`)
/// and spawns the first available player detached from the terminal.
pub fn launch_player(url: &str) -> ActionOutcome {
    let players = ["mpv", "vlc", "celluloid", "smplayer", "ffplay"];
    for player in players {
        let spawn_res = Command::new(player)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        if spawn_res.is_ok() {
            return ActionOutcome::PlayerLaunched(player.to_string());
        }
    }

    ActionOutcome::Error("No supported media player found (install mpv or vlc)".to_string())
}
