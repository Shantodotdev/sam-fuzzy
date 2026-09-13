//! Cyberpunk theme definition matching Blacksparrow terminal aesthetics.
//!
//! Provides color constants and reusable Ratatui styles based on hot pink
//! (ANSI 198), deep maroon (ANSI 161), and high-visibility neon accents.

use ratatui::style::{Color, Modifier, Style};

/// Hot Pink accent color matching Blacksparrow's primary brand color (ANSI 198).
pub const COLOR_PINK: Color = Color::Rgb(255, 0, 128);

/// Deep Maroon for borders, pills, and secondary structural elements (ANSI 161).
pub const COLOR_MAROON: Color = Color::Rgb(199, 0, 95);

/// Neon Green for success states, latency telemetry, and video file badges (ANSI 48).
pub const COLOR_NEON_GREEN: Color = Color::Rgb(0, 255, 128);

/// Cyan for resolution tags and streaming URLs.
pub const COLOR_CYAN: Color = Color::Rgb(0, 220, 255);

/// Bright Yellow for highlighting matched characters in search results (ANSI 220).
pub const COLOR_YELLOW: Color = Color::Rgb(255, 215, 0);

/// Red for error notifications (ANSI 196).
pub const COLOR_RED: Color = Color::Rgb(255, 60, 60);

/// Muted Dim Gray for breadcrumbs and secondary metadata.
pub const COLOR_DIM: Color = Color::Rgb(120, 120, 135);

/// Bright White for titles and active text.
pub const COLOR_WHITE: Color = Color::Rgb(250, 250, 255);

/// Dark panel background for contrast.
pub const COLOR_BG_PANEL: Color = Color::Rgb(20, 20, 28);

/// Subtle dark magenta background tint for the currently selected list row.
pub const COLOR_BG_SELECT: Color = Color::Rgb(48, 16, 40);

/// Primary header and title style.
pub fn style_header() -> Style {
    Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)
}

/// Standard unfocused box border style.
pub fn style_border() -> Style {
    Style::default().fg(COLOR_MAROON)
}

/// Focused border style (e.g. search input bar).
pub fn style_border_focused() -> Style {
    Style::default().fg(COLOR_PINK).add_modifier(Modifier::BOLD)
}

/// Highlighted row in the search results list.
pub fn style_selected_row() -> Style {
    Style::default()
        .fg(COLOR_WHITE)
        .bg(COLOR_BG_SELECT)
        .add_modifier(Modifier::BOLD)
}

/// Neon green badge style.
pub fn style_badge_green() -> Style {
    Style::default()
        .fg(COLOR_NEON_GREEN)
        .add_modifier(Modifier::BOLD)
}

/// Cyan badge style.
pub fn style_badge_cyan() -> Style {
    Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)
}

/// Yellow badge style.
pub fn style_badge_yellow() -> Style {
    Style::default()
        .fg(COLOR_YELLOW)
        .add_modifier(Modifier::BOLD)
}

/// Muted secondary text style.
pub fn style_dim() -> Style {
    Style::default().fg(COLOR_DIM)
}

/// Character highlight style inside matched titles.
pub fn style_highlight_match() -> Style {
    Style::default()
        .fg(COLOR_YELLOW)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

/// Style for result list row index numbers.
///
/// Uses high-visibility cyan to visually separate the item sequence number
/// from the white media title without visual clutter.
pub fn style_index(is_selected: bool) -> Style {
    if is_selected {
        Style::default().fg(COLOR_CYAN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(COLOR_CYAN)
    }
}
