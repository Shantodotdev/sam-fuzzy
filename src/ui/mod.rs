//! Terminal UI layout system and widget orchestration.
//!
//! Organizes widgets into a responsive layout tree adapting dynamically to
//! terminal dimensions:
//! - Banner & telemetry at the top (scales between full ASCII art and compact bar).
//! - Centered search bar with fzf-style instant feedback.
//! - Horizontal category navigation tabs.
//! - Dual-pane workspace (results list + metadata inspector) with toggleable sidebar (`Alt+S` / `F5`),
//!   responsively scaling across wide and narrow terminal widths.
//! - Expandable downloads panel above global status notifications and keybindings.

pub mod banner;
pub mod components;
pub mod theme;

use crate::app::App;
use banner::BannerWidget;
use components::{
    CategoryTabsWidget, DownloadsWidget, FooterWidget, InspectorWidget, ResultListWidget,
    SearchBarWidget,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

/// Primary UI render entrypoint invoked on every terminal redraw frame.
///
/// Computes responsive layout splits based on current terminal dimensions and
/// passes state snapshots to the respective widget renderers.
pub fn render_ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // Responsive footer height:
    // On wide terminals (>= 120 columns), all shortcuts fit on a single row.
    // Keep download-pane controls in a dedicated third footer row while that pane is visible.
    // Otherwise retain the compact one/two-row footer behavior.
    let footer_height = if app.show_downloads {
        3 + u16::from(app.active_status().is_some())
    } else if size.width >= 120 {
        1
    } else {
        2
    };
    // Keep the main search workspace at least eight rows tall. Any spare vertical space is
    // available to the downloads panel, which adds four rows per visible transfer.
    let fixed_height = 1 + 3 + 1 + footer_height + 8;
    let downloads_budget = size.height.saturating_sub(fixed_height);
    let downloads_height = app.downloads_panel_height(downloads_budget);

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                // Compact Header Banner
            Constraint::Length(3),                // Search Bar (input + rounded border)
            Constraint::Length(1),                // Category navigation pills
            Constraint::Min(8),                   // Results & Inspector workspace
            Constraint::Length(downloads_height), // Expandable downloads manager
            Constraint::Length(footer_height),    // Footer status & keybindings
        ])
        .split(size);

    // 1. Top Header Banner
    f.render_widget(BannerWidget { app }, vertical_chunks[0]);

    // 2. Interactive Search Bar
    f.render_widget(SearchBarWidget { app }, vertical_chunks[1]);

    // 3. Category Filter Tabs
    f.render_widget(CategoryTabsWidget { app }, vertical_chunks[2]);

    // 4. Central Workspace:
    // Responsively decides whether to display the inspector sidebar:
    // Default is open on wide terminals (>= 100 columns) and collapsed on narrow terminals (< 100 columns),
    // but users can toggle it open or closed at any screen size via keyboard shortcut.
    if app.is_sidebar_visible(size.width) {
        // Responsively adapt horizontal proportions based on terminal width:
        // - Wide screens (>= 100 cols): 58% Results, 42% Inspector
        // - Medium screens (80..99 cols): 54% Results, 46% Inspector
        // - Narrow screens (60..79 cols): 50% Results, 50% Inspector
        // - Ultra-narrow screens (< 60 cols): 48% Results, 52% Inspector
        let (results_pct, inspector_pct) = if size.width >= 100 {
            (58, 42)
        } else if size.width >= 80 {
            (54, 46)
        } else if size.width >= 60 {
            (50, 50)
        } else {
            (48, 52)
        };

        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(results_pct),   // Search Results List
                Constraint::Percentage(inspector_pct), // Inspector Preview & Details
            ])
            .split(vertical_chunks[3]);

        f.render_widget(ResultListWidget { app }, horizontal_chunks[0]);
        f.render_widget(InspectorWidget { app }, horizontal_chunks[1]);
    } else {
        // Single-pane fallback: dedicate full workspace width to Results List
        f.render_widget(ResultListWidget { app }, vertical_chunks[3]);
    }

    // 5. Download manager is always below the search workspace, never obscuring its input.
    if downloads_height > 0 {
        f.render_widget(DownloadsWidget { app }, vertical_chunks[4]);
    }

    // 6. Bottom Status and Keybindings Ribbon
    f.render_widget(FooterWidget { app }, vertical_chunks[5]);
}
