//! Central application state machine.
//!
//! Tracks interactive input, result pagination, category cycling, and
//! coordinates action dispatches between the fuzzy engine and terminal UI.

use crate::actions::{copy_to_clipboard, launch_player, open_in_browser, ActionOutcome};
use crate::fuzzy::SearchEngine;
use crate::models::MediaItem;
use std::time::{Duration, Instant};

/// Available category navigation tabs displayed at the top of the interface.
pub const CATEGORIES: &[&str] = &[
    "All",
    "English",
    "TV Series",
    "Korean",
    "Hindi",
    "Animation",
    "1080p",
    "720p",
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

/// Main application state holder.
pub struct App {
    /// In-memory fuzzy search engine.
    pub engine: SearchEngine,

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

    /// Signals the main event loop to terminate cleanly.
    pub should_quit: bool,
}

impl App {
    /// Creates a new application state and executes an initial empty search
    /// to populate the default view with all available items.
    pub fn new(engine: SearchEngine) -> Self {
        let mut app = Self {
            engine,
            query: String::new(),
            selected_index: 0,
            category_index: 0,
            results: Vec::new(),
            search_latency: Duration::ZERO,
            status: None,
            show_help: false,
            should_quit: false,
        };
        app.perform_search();
        app
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
        if let Some(pos) = CATEGORIES.iter().position(|&c| c.eq_ignore_ascii_case(name)) {
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

    /// Executes search across the index with active query and category filters,
    /// recording query latency and clamping selection bounds.
    pub fn perform_search(&mut self) {
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
        if let Some((msg, time, is_err)) = &self.status {
            if time.elapsed() < Duration::from_secs(4) {
                return Some((msg.as_str(), *is_err));
            }
        }
        None
    }

    /// Toggles the keyboard help modal.
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }
}
