//! # sam-fuzzy
//!
//! A fast, cyberpunk-styled terminal user interface (TUI) for searching and
//! streaming media from SamOnline (DhakaFlix) FTP servers.
//!
//! ## Architecture
//! - [`models`]: Core media structures, category tagging, and JSON dataset loaders.
//! - [`fuzzy`]: High-speed fuzzy search engine using `nucleo-matcher` with exact-match tier bonuses.
//! - [`actions`]: External handlers to open links in browsers, copy to clipboard, or stream via players.
//! - [`app`]: Central state machine handling keyboard events, category filtering, and selection.
//! - [`ui`]: Terminal rendering components using `ratatui` with Blacksparrow cyberpunk aesthetics.

pub mod actions;
pub mod app;
pub mod fuzzy;
pub mod models;
pub mod ui;
