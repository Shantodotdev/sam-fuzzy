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

    /// Action failed with an error description.
    Error(String),
}

impl ActionOutcome {
    /// User-facing status line notification.
    pub fn message(&self) -> String {
        match self {
            ActionOutcome::BrowserOpened(url) => format!("🌐 Opened in browser: {}", url),
            ActionOutcome::CopiedToClipboard(url) => format!("📋 Copied link to clipboard: {}", url),
            ActionOutcome::PlayerLaunched(player) => format!("🎬 Launched {} player in background", player),
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

use std::io::Write;
use std::sync::Mutex;

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
        ActionOutcome::Error("Could not access system clipboard (tried wl-copy, xclip, xsel, arboard)".to_string())
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
