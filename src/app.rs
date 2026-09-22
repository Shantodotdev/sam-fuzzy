//! Application state machine and event handling.
//!
//! Encapsulates the user interface state including:
//! - Search query buffer and cursor tracking.
//! - Category filter navigation tabs.
//! - Selection indices and bounds clamping.
//! - Non-blocking background search worker thread management.
//! - Global actions dispatch (browser launch, clipboard copy, video playback).

use crate::actions::{
    ActionOutcome, DownloadEvent, copy_to_clipboard, default_download_dir, launch_player,
    open_in_browser, start_download,
};
use crate::fuzzy::SearchEngine;
use crate::models::MediaItem;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

/// Available category filter tabs.
pub const CATEGORIES: &[&str] = &[
    "All",
    "English",
    "TV Series",
    "Korean",
    "Hindi",
    "Animation",
];

/// Maximum number of search matches kept in memory for interactive scrolling.
pub const MAX_VISIBLE_RESULTS: usize = 300;

/// Render-ready item containing fuzzy score and character highlight indices.
#[derive(Debug, Clone)]
pub struct DisplayResult {
    pub item: MediaItem,
    pub score: u64,
    pub indices: Vec<u32>,
}

/// Current lifecycle state of a managed file download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadState {
    /// The worker was created and is connecting to the source.
    Queued,
    /// Bytes are currently arriving from the source server.
    Downloading,
    /// The completed file was atomically moved into the selected download directory.
    Completed,
    /// The transfer stopped before completion.
    Failed(String),
}

/// Render-ready state for one file download in the bottom downloads panel.
#[derive(Debug, Clone)]
pub struct DownloadTask {
    /// Stable ID for matching worker updates to this task.
    pub id: u64,
    /// Source filename shown in the TUI.
    pub filename: String,
    /// Final path after the transfer completes.
    pub destination: PathBuf,
    /// Current transfer lifecycle state.
    pub state: DownloadState,
    /// Number of bytes written so far.
    pub downloaded: u64,
    /// Expected full size, when exposed by the server.
    pub total: Option<u64>,
    /// Smoothed most-recent transfer rate in bytes/sec.
    pub bytes_per_second: f64,
    last_sample_bytes: u64,
    last_sample_at: Instant,
}

impl DownloadTask {
    /// Creates a newly queued task before its background worker reports progress.
    pub fn queued(id: u64, filename: String, destination: PathBuf) -> Self {
        Self {
            id,
            filename,
            destination,
            state: DownloadState::Queued,
            downloaded: 0,
            total: None,
            bytes_per_second: 0.0,
            last_sample_bytes: 0,
            last_sample_at: Instant::now(),
        }
    }
}

/// Internal request payload sent to the background search thread.
struct SearchRequest {
    id: u64,
    query: String,
    category: &'static str,
}

/// Internal response payload received from the background search thread.
struct SearchResponse {
    id: u64,
    results: Vec<DisplayResult>,
    latency: Duration,
}

/// Main application state holder.
pub struct App {
    /// In-memory fuzzy search engine (wrapped in Arc for thread-safe concurrent access).
    pub engine: Arc<SearchEngine>,

    /// Current search query string typed by the user.
    pub query: String,

    /// Index of the currently highlighted item in `results`.
    pub selected_index: usize,

    /// Index of the currently active category in `CATEGORIES`.
    pub category_index: usize,

    /// Filtered and ranked results for the active query and category.
    pub results: Vec<DisplayResult>,

    /// Execution duration of the last fuzzy search operation.
    pub search_latency: Duration,

    /// Ephemeral status message (text, creation instant, is_error).
    pub status: Option<(String, Instant, bool)>,

    /// Toggle flag for the keyboard help modal.
    pub show_help: bool,

    /// Whether the bottom downloads progress panel is visible.
    pub show_downloads: bool,

    /// User-configured target directory for downloaded media.
    pub download_dir: PathBuf,

    /// Recent and active background transfers displayed by the downloads panel.
    pub downloads: Vec<DownloadTask>,

    /// Explicit override for sidebar (inspector) visibility.
    /// `None` indicates responsive auto-visibility (visible when width >= 100).
    /// `Some(true)` or `Some(false)` indicates user-toggled manual override.
    pub show_sidebar: Option<bool>,

    /// Signals the main event loop to terminate cleanly.
    pub should_quit: bool,

    // Channels for non-blocking search worker
    search_tx: Option<Sender<SearchRequest>>,
    search_rx: Option<Receiver<SearchResponse>>,
    request_counter: u64,
    pending_request_id: u64,
    download_tx: Sender<DownloadEvent>,
    download_rx: Receiver<DownloadEvent>,
    download_counter: u64,
}

impl App {
    /// Creates a new application state and executes an initial empty search
    /// to populate the default view with all available items.
    pub fn new(engine: SearchEngine) -> Self {
        let (download_tx, download_rx) = channel::<DownloadEvent>();
        let mut app = Self {
            engine: Arc::new(engine),
            query: String::new(),
            selected_index: 0,
            category_index: 0,
            results: Vec::new(),
            search_latency: Duration::ZERO,
            status: None,
            show_help: false,
            show_downloads: false,
            download_dir: default_download_dir(),
            downloads: Vec::new(),
            show_sidebar: None,
            should_quit: false,
            search_tx: None,
            search_rx: None,
            request_counter: 0,
            pending_request_id: 0,
            download_tx,
            download_rx,
            download_counter: 0,
        };
        app.perform_search_sync();
        app
    }

    /// Sets the directory where new downloads are written.
    pub fn set_download_dir(&mut self, directory: PathBuf) {
        self.download_dir = directory;
    }

    /// Applies any queued file-transfer updates without blocking UI input or rendering.
    pub fn poll_downloads(&mut self) {
        while let Ok(event) = self.download_rx.try_recv() {
            match event {
                DownloadEvent::Progress {
                    id,
                    downloaded,
                    total,
                } => {
                    if let Some(task) = self.downloads.iter_mut().find(|task| task.id == id) {
                        let elapsed = task.last_sample_at.elapsed().as_secs_f64();
                        if elapsed > 0.0 {
                            task.bytes_per_second =
                                downloaded.saturating_sub(task.last_sample_bytes) as f64 / elapsed;
                        }
                        task.downloaded = downloaded;
                        task.total = total.or(task.total);
                        task.last_sample_bytes = downloaded;
                        task.last_sample_at = Instant::now();
                        task.state = DownloadState::Downloading;
                    }
                }
                DownloadEvent::Completed {
                    id,
                    destination,
                    downloaded,
                } => {
                    if let Some(task) = self.downloads.iter_mut().find(|task| task.id == id) {
                        task.destination = destination;
                        task.downloaded = downloaded;
                        task.total = Some(downloaded);
                        task.bytes_per_second = 0.0;
                        task.state = DownloadState::Completed;
                    }
                }
                DownloadEvent::Failed { id, error } => {
                    if let Some(task) = self.downloads.iter_mut().find(|task| task.id == id) {
                        task.bytes_per_second = 0.0;
                        task.state = DownloadState::Failed(error);
                    }
                }
            }
        }
    }

    /// Starts downloading the selected file and opens the live bottom downloads panel.
    ///
    /// Directories are deliberately rejected because their server listings do not identify a
    /// single file to save.
    pub fn download_selected(&mut self) -> Option<ActionOutcome> {
        let item = self.selected_item()?.clone();
        if !item.is_file {
            let outcome = ActionOutcome::Error("Select a media file to download.".to_string());
            self.set_status(&outcome.message(), true);
            return Some(outcome);
        }

        if self.downloads.iter().any(|task| {
            task.filename == item.filename
                && matches!(
                    task.state,
                    DownloadState::Queued | DownloadState::Downloading
                )
        }) {
            self.show_downloads = true;
            let outcome = ActionOutcome::DownloadAlreadyQueued(item.filename);
            self.set_status(&outcome.message(), false);
            return Some(outcome);
        }

        self.download_counter = self.download_counter.saturating_add(1);
        let id = self.download_counter;
        let filename = item.filename.clone();
        match start_download(
            id,
            item.url,
            filename.clone(),
            self.download_dir.clone(),
            self.download_tx.clone(),
        ) {
            Ok(destination) => {
                self.downloads
                    .push(DownloadTask::queued(id, filename, destination.clone()));
                self.show_downloads = true;
                let outcome = ActionOutcome::DownloadStarted(destination);
                self.set_status(&outcome.message(), false);
                Some(outcome)
            }
            Err(error) => {
                let outcome = ActionOutcome::Error(format!("Could not start download: {error}"));
                self.set_status(&outcome.message(), true);
                Some(outcome)
            }
        }
    }

    /// Toggles the bottom downloads progress panel.
    pub fn toggle_downloads(&mut self) {
        self.show_downloads = !self.show_downloads;
    }

    /// Preferred height for the expandable downloads panel.
    ///
    /// Each visible download consumes four rows (state/name, progress bar,
    /// transfer details, spacer); the panel stops growing when `max_height`
    /// would otherwise take space reserved for search results.
    pub fn downloads_panel_height(&self, max_height: u16) -> u16 {
        if !self.show_downloads {
            return 0;
        }

        let task_rows = self.downloads.len().max(1).saturating_mul(4) as u16;
        (task_rows + 2).min(max_height)
    }

    /// Number of queued or actively transferring files.
    pub fn active_download_count(&self) -> usize {
        self.downloads
            .iter()
            .filter(|task| {
                matches!(
                    task.state,
                    DownloadState::Queued | DownloadState::Downloading
                )
            })
            .count()
    }

    /// Spawns a dedicated background search worker thread communicating via lock-free channels.
    ///
    /// Offloads all fuzzy matching from the UI thread so that typing and rendering
    /// run at a locked 60+ FPS with zero input lag.
    pub fn spawn_search_worker(&mut self) {
        let (req_tx, req_rx) = channel::<SearchRequest>();
        let (res_tx, res_rx) = channel::<SearchResponse>();

        let engine = Arc::clone(&self.engine);

        thread::Builder::new()
            .name("search-worker".to_string())
            .spawn(move || {
                while let Ok(req) = req_rx.recv() {
                    // Drain any subsequent requests that accumulated while busy,
                    // keeping only the latest query to avoid wasted work on stale keystrokes.
                    let mut latest_req = req;
                    while let Ok(newer_req) = req_rx.try_recv() {
                        latest_req = newer_req;
                    }

                    let start = Instant::now();
                    let matches =
                        engine.search(&latest_req.query, latest_req.category, MAX_VISIBLE_RESULTS);
                    let latency = start.elapsed();

                    let display_results: Vec<DisplayResult> = matches
                        .into_iter()
                        .map(|m| DisplayResult {
                            item: m.item.clone(),
                            score: m.score,
                            indices: m.indices,
                        })
                        .collect();

                    if res_tx
                        .send(SearchResponse {
                            id: latest_req.id,
                            results: display_results,
                            latency,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("Failed to spawn background search worker thread");

        self.search_tx = Some(req_tx);
        self.search_rx = Some(res_rx);
    }

    /// Non-blocking check for completed search results from the background worker.
    ///
    /// Called on every frame in the main event loop to apply latest results without delay.
    pub fn poll_search_results(&mut self) {
        if let Some(rx) = &self.search_rx {
            let mut latest = None;
            while let Ok(resp) = rx.try_recv() {
                if resp.id >= self.pending_request_id {
                    latest = Some(resp);
                }
            }
            if let Some(resp) = latest {
                self.results = resp.results;
                self.search_latency = resp.latency;
                if self.selected_index >= self.results.len() {
                    self.selected_index = self.results.len().saturating_sub(1);
                }
            }
        }
    }

    /// Label of the currently active category tab.
    pub fn current_category(&self) -> &'static str {
        CATEGORIES[self.category_index]
    }

    /// Cycles forward to the next category tab.
    pub fn next_category(&mut self) {
        self.category_index = (self.category_index + 1) % CATEGORIES.len();
        self.selected_index = 0;
        self.perform_search();
    }

    /// Cycles backward to the previous category tab.
    pub fn prev_category(&mut self) {
        if self.category_index == 0 {
            self.category_index = CATEGORIES.len() - 1;
        } else {
            self.category_index -= 1;
        }
        self.selected_index = 0;
        self.perform_search();
    }

    /// Selects a specific category by name (case-insensitive).
    pub fn set_category_by_name(&mut self, name: &str) {
        if let Some(pos) = CATEGORIES
            .iter()
            .position(|&c| c.eq_ignore_ascii_case(name))
        {
            self.category_index = pos;
            self.selected_index = 0;
            self.perform_search();
        }
    }

    /// Appends a typed character to the search query and refreshes results.
    pub fn on_key_char(&mut self, c: char) {
        self.query.push(c);
        self.selected_index = 0;
        self.perform_search();
    }

    /// Removes the trailing character from the query.
    pub fn on_backspace(&mut self) {
        if self.query.pop().is_some() {
            self.selected_index = 0;
            self.perform_search();
        }
    }

    /// Clears the search query entirely.
    pub fn on_clear_query(&mut self) {
        if !self.query.is_empty() {
            self.query.clear();
            self.selected_index = 0;
            self.perform_search();
        }
    }

    /// Moves selection down by one row, clamping to available results.
    pub fn select_next(&mut self) {
        if !self.results.is_empty() && self.selected_index + 1 < self.results.len() {
            self.selected_index += 1;
        }
    }

    /// Moves selection up by one row, clamping at index 0.
    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Advances selection by one full page.
    pub fn select_next_page(&mut self, page_size: usize) {
        if !self.results.is_empty() {
            self.selected_index = (self.selected_index + page_size).min(self.results.len() - 1);
        }
    }

    /// Rewinds selection by one full page.
    pub fn select_prev_page(&mut self, page_size: usize) {
        if self.selected_index >= page_size {
            self.selected_index -= page_size;
        } else {
            self.selected_index = 0;
        }
    }

    /// Jumps directly to the first result.
    pub fn select_first(&mut self) {
        self.selected_index = 0;
    }

    /// Jumps directly to the last result.
    pub fn select_last(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = self.results.len() - 1;
        }
    }

    /// Reference to the currently highlighted item, if any.
    pub fn selected_item(&self) -> Option<&MediaItem> {
        self.results.get(self.selected_index).map(|r| &r.item)
    }

    /// Dispatches a search. If the background worker is running, sends an asynchronous
    /// non-blocking request (taking 0.001ms). Otherwise, executes search synchronously.
    pub fn perform_search(&mut self) {
        if let Some(tx) = &self.search_tx {
            self.request_counter += 1;
            let id = self.request_counter;
            self.pending_request_id = id;
            let _ = tx.send(SearchRequest {
                id,
                query: self.query.clone(),
                category: self.current_category(),
            });
        } else {
            self.perform_search_sync();
        }
    }

    /// Executes search synchronously across the index.
    pub fn perform_search_sync(&mut self) {
        let start = Instant::now();
        let cat = self.current_category();
        let matches = self.engine.search(&self.query, cat, MAX_VISIBLE_RESULTS);
        self.search_latency = start.elapsed();

        self.results = matches
            .into_iter()
            .map(|m| DisplayResult {
                item: m.item.clone(),
                score: m.score,
                indices: m.indices,
            })
            .collect();

        if self.selected_index >= self.results.len() {
            self.selected_index = self.results.len().saturating_sub(1);
        }
    }

    /// Action: Opens the selected item in the default web browser.
    ///
    /// Triggered by `Enter`. Direct files open stream/download URLs; directories
    /// open the h5ai folder listing on the server mirror.
    pub fn open_selected_in_browser(&mut self) -> Option<ActionOutcome> {
        if let Some(item) = self.selected_item() {
            let target_url = if item.is_file {
                &item.url
            } else {
                &item.folder_url
            };
            let outcome = open_in_browser(target_url);
            self.set_status(&outcome.message(), outcome.is_error());
            Some(outcome)
        } else {
            None
        }
    }

    /// Action: Opens the parent directory / folder in the browser.
    ///
    /// Triggered by `Alt+F` or `F4`.
    pub fn open_folder_in_browser(&mut self) -> Option<ActionOutcome> {
        if let Some(item) = self.selected_item() {
            let outcome = open_in_browser(&item.folder_url);
            self.set_status(&outcome.message(), outcome.is_error());
            Some(outcome)
        } else {
            None
        }
    }

    /// Action: Copies the direct link to the clipboard.
    ///
    /// Triggered by `Alt+C`, `F2`, or `Ctrl+Y`.
    pub fn copy_selected_link(&mut self) -> Option<ActionOutcome> {
        if let Some(item) = self.selected_item() {
            let target_url = if item.is_file {
                &item.url
            } else {
                &item.folder_url
            };
            let outcome = copy_to_clipboard(target_url);
            self.set_status(&outcome.message(), outcome.is_error());
            Some(outcome)
        } else {
            None
        }
    }

    /// Action: Streams the selected video directly in MPV / VLC.
    ///
    /// Triggered by `Alt+P` or `F3`.
    pub fn play_selected_video(&mut self) -> Option<ActionOutcome> {
        if let Some(item) = self.selected_item() {
            let target_url = if item.is_file {
                &item.url
            } else {
                &item.folder_url
            };
            let outcome = launch_player(target_url);
            self.set_status(&outcome.message(), outcome.is_error());
            Some(outcome)
        } else {
            None
        }
    }

    /// Sets an ephemeral notification message shown in the footer.
    pub fn set_status(&mut self, message: &str, is_error: bool) {
        self.status = Some((message.to_string(), Instant::now(), is_error));
    }

    /// Returns the active status message if within its 4-second TTL.
    pub fn active_status(&self) -> Option<(&str, bool)> {
        match &self.status {
            Some((msg, time, is_err)) if time.elapsed() < Duration::from_secs(4) => {
                Some((msg.as_str(), *is_err))
            }
            _ => None,
        }
    }

    /// Toggles the keyboard help modal.
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Toggles the right-side inspector sidebar visibility using the current terminal width.
    pub fn toggle_sidebar(&mut self) {
        let width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(100);
        self.toggle_sidebar_with_width(width);
    }

    /// Toggles the right-side inspector sidebar visibility based on an explicit width.
    pub fn toggle_sidebar_with_width(&mut self, width: u16) {
        let currently_visible = self.is_sidebar_visible(width);
        self.show_sidebar = Some(!currently_visible);
    }

    /// Determines whether the sidebar should currently be rendered given terminal width.
    pub fn is_sidebar_visible(&self, width: u16) -> bool {
        self.show_sidebar.unwrap_or(width >= 100)
    }
}
