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

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
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

/// Starts a resumable HTTP(S) file transfer on a detached worker thread.
///
/// Downloads are written to `<name>.part` first. If a prior partial file exists,
/// the worker requests the remaining range and continues it when the server supports
/// HTTP range requests. The final rename only happens after the full response is written.
pub fn start_download(
    id: u64,
    url: String,
    filename: String,
    directory: PathBuf,
    updates: Sender<DownloadEvent>,
    cancellation: Arc<AtomicBool>,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(&directory)?;
    let destination = available_destination(&directory, &filename);
    let part_path = part_path_for(&destination);
    let worker_destination = destination.clone();

    thread::Builder::new()
        .name(format!("download-{id}"))
        .spawn(move || {
            match download_to_path(
                id,
                &url,
                &part_path,
                &worker_destination,
                &updates,
                &cancellation,
            ) {
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

fn download_to_path(
    id: u64,
    url: &str,
    part_path: &Path,
    destination: &Path,
    updates: &Sender<DownloadEvent>,
    cancellation: &AtomicBool,
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
        .map(|remaining| remaining.saturating_add(starting_bytes));

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
