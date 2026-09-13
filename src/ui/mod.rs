//! Terminal UI layout system and widget orchestration.
//!
//! Organizes widgets into a responsive layout tree adapting dynamically to
//! terminal dimensions:
//! - Banner & telemetry at the top (scales between full ASCII art and compact bar).
//! - Centered search bar with fzf-style instant feedback.
//! - Horizontal category navigation tabs.
//! - Dual-pane workspace (results list + metadata inspector) on wide screens (>= 100 columns),
//!   collapsing to a single full-width results pane on narrow terminals.
//! - Global status notifications and keybindings footer.

pub mod banner;
pub mod components;
pub mod theme;

use crate::app::App;
use banner::BannerWidget;
use components::{
    CategoryTabsWidget, FooterWidget, InspectorWidget, ResultListWidget, SearchBarWidget,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

/// Primary UI render entrypoint invoked on every terminal redraw frame.
///
/// Computes responsive layout splits based on current terminal dimensions and
/// passes state snapshots to the respective widget renderers.
pub fn render_ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // Primary vertical layout allocation:
    // Uses a streamlined 1-line header to maximize vertical screen space for results.
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Compact Header Banner
            Constraint::Length(3), // Search Bar (input + rounded border)
            Constraint::Length(1), // Category navigation pills
            Constraint::Min(8),    // Results & Inspector workspace
            Constraint::Length(1), // Footer status & keybindings
        ])
        .split(size);

    // 1. Top Header Banner
    f.render_widget(BannerWidget { app }, vertical_chunks[0]);

    // 2. Interactive Search Bar
    f.render_widget(SearchBarWidget { app }, vertical_chunks[1]);

    // 3. Category Filter Tabs
    f.render_widget(CategoryTabsWidget { app }, vertical_chunks[2]);

    // 4. Central Workspace:
    // On wide terminals (>= 100 columns), split horizontally into Results (58%) and Inspector (42%).
    // On narrow terminals (< 100 columns), dedicate full width to Results for readability.
    if size.width >= 100 {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(58), // Search Results List
                Constraint::Percentage(42), // Inspector Preview & Cheatsheet
            ])
            .split(vertical_chunks[3]);

        f.render_widget(ResultListWidget { app }, horizontal_chunks[0]);
        f.render_widget(InspectorWidget { app }, horizontal_chunks[1]);
    } else {
        // Compact single-pane fallback
        f.render_widget(ResultListWidget { app }, vertical_chunks[3]);
    }

    // 5. Bottom Status and Keybindings Ribbon
    f.render_widget(FooterWidget { app }, vertical_chunks[4]);
}
