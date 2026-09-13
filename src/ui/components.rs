//! UI components for search input, category tabs, result listing, inspector, and footer.
//!
//! Each widget implements Ratatui's [`Widget`] trait, drawing directly into
//! the frame buffer with zero allocations in tight render loops.

use crate::app::{App, CATEGORIES};
use crate::ui::theme::*;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

/// Interactive search input bar with real-time prompt and cursor.
///
/// Shows a helpful placeholder when the query buffer is empty, and renders
/// the active query text alongside a solid cursor glyph (`█`) when typing.
pub struct SearchBarWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for SearchBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_border_focused())
            .title(Span::styled(" FUZZY SEARCH (fzf-style) ", style_header()));

        let inner_area = block.inner(area);
        block.render(area, buf);

        // Render helpful placeholder or active query with trailing cursor
        let query_text = if self.app.query.is_empty() {
            vec![
                Span::styled("❯ ", style_header()),
                Span::styled("Type to fuzzy search movies, series, years, resolutions... (e.g. 'witcher 2025', 'dark knight 1080p')", style_dim()),
            ]
        } else {
            vec![
                Span::styled("❯ ", style_header()),
                Span::styled(&self.app.query, Style::default().fg(COLOR_WHITE).add_modifier(Modifier::BOLD)),
                Span::styled("█", style_header()), // Active text cursor
            ]
        };

        Paragraph::new(Line::from(query_text)).render(inner_area, buf);
    }
}

/// Category navigation tabs displayed as horizontal pills.
///
/// Highlights the currently selected category with a solid maroon badge and
/// indicates navigation shortcuts (`Tab` and `Shift+Tab`).
pub struct CategoryTabsWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for CategoryTabsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans = vec![
            Span::styled(" CATEGORY: ", style_dim()),
        ];

        // Render each category as a selectable pill
        for (i, &cat) in CATEGORIES.iter().enumerate() {
            if i == self.app.category_index {
                // Active tab: bold white text over maroon background
                spans.push(Span::styled(
                    format!(" ◆ {} ", cat.to_uppercase()),
                    Style::default().fg(COLOR_WHITE).bg(COLOR_MAROON).add_modifier(Modifier::BOLD),
                ));
            } else {
                // Inactive tabs: dimmed text
                spans.push(Span::styled(
                    format!("   {}   ", cat),
                    Style::default().fg(COLOR_DIM),
                ));
            }
            spans.push(Span::raw(" "));
        }

        spans.push(Span::styled(" [Tab / Shift+Tab to switch]", style_dim()));

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}

/// Virtualized list widget displaying ranked fuzzy search matches.
///
/// Features:
/// - Virtualized viewport scrolling: only renders rows visible on screen.
/// - Centered selection tracking: keeps the active item vertically centered.
/// - Substring match highlighting: underlines and colors matched characters.
/// - Type badges: displays color-coded badges (`MKV`, `MP4`, `DIR`).
pub struct ResultListWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for ResultListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Title includes 1-based position and total match count
        let count_title = format!(
            " RESULTS ({}/{}) ",
            if self.app.results.is_empty() { 0 } else { self.app.selected_index + 1 },
            self.app.results.len()
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_border())
            .title(Span::styled(count_title, style_header()));

        let inner_area = block.inner(area);
        block.render(area, buf);

        // Empty state when search yields no matches
        if self.app.results.is_empty() {
            let empty_msg = Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled("  No matching media found on SamOnline.", style_dim())),
                Line::from(Span::styled("  Try clearing or simplifying your fuzzy query.", style_dim())),
            ]);
            empty_msg.render(inner_area, buf);
            return;
        }

        let visible_height = inner_area.height as usize;
        let selected = self.app.selected_index;

        // Virtualized scroll window: keep selected item centered in the viewport.
        // Clamps appropriately when near the top or bottom of the results list.
        let scroll_top = if selected < visible_height / 2 {
            0
        } else if selected + visible_height / 2 >= self.app.results.len() {
            self.app.results.len().saturating_sub(visible_height)
        } else {
            selected.saturating_sub(visible_height / 2)
        };

        let visible_items = self.app.results.iter().skip(scroll_top).take(visible_height);

        let mut lines = Vec::new();
        for (rel_idx, res) in visible_items.enumerate() {
            let current_idx = scroll_top + rel_idx;
            let is_selected = current_idx == selected;

            let mut line_spans = Vec::new();

            // Row selection indicator glyph
            if is_selected {
                line_spans.push(Span::styled(" ▶ ", style_header()));
            } else {
                line_spans.push(Span::styled("   ", style_dim()));
            }

            // File type badge: Green for playable video files, Cyan for directories
            let type_badge = res.item.file_type_label();
            let type_style = if res.item.is_file {
                style_badge_green()
            } else {
                style_badge_cyan()
            };
            line_spans.push(Span::styled(format!("{:<4} ", type_badge), type_style));

            // Formatted title with optional release year
            let display_title = res.item.display_title();
            let title_style = if is_selected {
                Style::default().fg(COLOR_WHITE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_WHITE)
            };

            // Character-level match highlighting.
            // Uses a HashSet for O(1) lookup per character to color matched runes.
            if res.indices.is_empty() {
                line_spans.push(Span::styled(display_title, title_style));
            } else {
                let chars: Vec<char> = display_title.chars().collect();
                let match_set: std::collections::HashSet<u32> = res.indices.iter().cloned().collect();

                for (char_idx, &ch) in chars.iter().enumerate() {
                    if match_set.contains(&(char_idx as u32)) {
                        line_spans.push(Span::styled(ch.to_string(), style_highlight_match()));
                    } else {
                        line_spans.push(Span::styled(ch.to_string(), title_style));
                    }
                }
            }

            // Quality tag (e.g. 1080p, 720p, 2160p)
            if !res.item.quality.is_empty() && res.item.quality != "Standard" {
                line_spans.push(Span::raw("  "));
                line_spans.push(Span::styled(format!("[{}]", res.item.quality), style_badge_cyan()));
            }

            // Category tag
            line_spans.push(Span::raw("  "));
            line_spans.push(Span::styled(format!("• {}", res.item.category), style_dim()));

            // Apply selected row background highlight
            let mut row = Line::from(line_spans);
            if is_selected {
                row = row.style(style_selected_row());
            }

            lines.push(row);
        }

        Paragraph::new(lines).render(inner_area, buf);
    }
}

/// Detailed metadata inspector panel and actions cheatsheet for the selected media item.
///
/// Displays full metadata breakdown including:
/// - Title, release year, and encoding quality profile.
/// - Section category and originating mirror server ID.
/// - Full folder hierarchy rendered as hierarchical breadcrumbs.
/// - Direct stream/download URL and parent directory URL.
/// - Quick-action keyboard cheatsheet.
pub struct InspectorWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for InspectorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_border())
            .title(Span::styled(" MEDIA INSPECTOR & ACTIONS ", style_header()));

        let inner_area = block.inner(area);
        block.render(area, buf);

        // Guard against rendering when no items match the current query
        let Some(item) = self.app.selected_item() else {
            let empty = Paragraph::new("No media selected.")
                .alignment(Alignment::Center)
                .style(style_dim());
            empty.render(inner_area, buf);
            return;
        };

        let mut lines = Vec::new();

        // 1. Primary Title
        lines.push(Line::from(vec![
            Span::styled(" TITLE    : ", style_header()),
            Span::styled(item.display_title(), Style::default().fg(COLOR_WHITE).add_modifier(Modifier::BOLD)),
        ]));

        // 2. Release Year & Quality Profile
        let year_str = item.year.map(|y| y.to_string()).unwrap_or_else(|| "N/A".to_string());
        lines.push(Line::from(vec![
            Span::styled(" RELEASE  : ", style_dim()),
            Span::styled(year_str, style_badge_green()),
            Span::styled("   QUALITY : ", style_dim()),
            Span::styled(&item.quality, style_badge_cyan()),
        ]));

        // 3. Category & Server Mirror Host
        lines.push(Line::from(vec![
            Span::styled(" SECTION  : ", style_dim()),
            Span::styled(&item.category, style_badge_yellow()),
            Span::styled("   SERVER  : ", style_dim()),
            Span::styled(&item.server, style_dim()),
        ]));

        lines.push(Line::raw(""));

        // 4. File Type and Physical Filename
        let type_desc = if item.is_file {
            "Direct Video File (.mkv / .mp4)"
        } else {
            "Media Directory / Series Folder"
        };
        lines.push(Line::from(vec![
            Span::styled(" TYPE     : ", style_dim()),
            Span::styled(type_desc, Style::default().fg(COLOR_WHITE)),
        ]));

        lines.push(Line::from(vec![
            Span::styled(" FILENAME : ", style_dim()),
            Span::styled(&item.filename, style_dim()),
        ]));

        lines.push(Line::raw(""));

        // 5. Breadcrumb Path: Renders hierarchy tree using ASCII branch connectors
        lines.push(Line::from(vec![
            Span::styled(" PATH BREADCRUMBS:", style_header()),
        ]));
        let path_parts = item.path.split('/').collect::<Vec<_>>();
        for (idx, part) in path_parts.iter().enumerate() {
            let prefix = if idx == path_parts.len() - 1 { " └─ " } else { " ├─ " };
            lines.push(Line::from(vec![
                Span::styled(prefix, style_dim()),
                Span::styled(*part, if idx == path_parts.len() - 1 { style_badge_green() } else { style_dim() }),
            ]));
        }

        lines.push(Line::raw(""));

        // 6. Direct HTTP Stream URL
        lines.push(Line::from(vec![
            Span::styled(" DIRECT STREAM URL:", style_header()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(format!(" {}", item.url), Style::default().fg(COLOR_CYAN).add_modifier(Modifier::UNDERLINED)),
        ]));

        lines.push(Line::raw(""));

        // 7. Parent Folder Mirror URL (opens directory view in browser)
        lines.push(Line::from(vec![
            Span::styled(" PARENT FOLDER URL:", style_header()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(format!(" {}", item.folder_url), Style::default().fg(COLOR_DIM)),
        ]));

        lines.push(Line::raw(""));

        // 8. Action Shortcuts Reference Box
        lines.push(Line::from(vec![
            Span::styled(" ┌──────────────── ACTION SHORTCUTS ────────────────┐", style_dim()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" │ ", style_dim()),
            Span::styled("[Enter]", style_header()),
            Span::styled(" Open in Browser (DhakaFlix / Stream)    │", Style::default().fg(COLOR_WHITE)),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" │ ", style_dim()),
            Span::styled("[Alt+C]", style_header()),
            Span::styled(" Copy Direct Link (or F2 / Ctrl+Y)        │", Style::default().fg(COLOR_WHITE)),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" │ ", style_dim()),
            Span::styled("[Alt+P]", style_header()),
            Span::styled(" Stream with MPV / VLC (or F3)            │", Style::default().fg(COLOR_WHITE)),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" │ ", style_dim()),
            Span::styled("[Alt+F]", style_header()),
            Span::styled(" Open Parent Folder in Browser (or F4)    │", Style::default().fg(COLOR_WHITE)),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" └──────────────────────────────────────────────────┘", style_dim()),
        ]));

        Paragraph::new(lines).render(inner_area, buf);
    }
}

/// Bottom status line displaying transient toast notifications and keybindings.
///
/// If an action was just triggered (e.g. copied to clipboard or opened in browser),
/// an ephemeral status message is displayed with a 4-second TTL. Otherwise, displays
/// the full global keyboard navigation guide.
pub struct FooterWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for FooterWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans = Vec::new();

        // Render transient notification badge if within its 4-second expiration window
        if let Some((status_msg, is_err)) = self.app.active_status() {
            let status_style = if is_err {
                Style::default().fg(COLOR_RED).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_NEON_GREEN).add_modifier(Modifier::BOLD)
            };
            spans.push(Span::styled(format!(" {} ", status_msg), status_style));
            spans.push(Span::styled(" │ ", style_dim()));
        }

        // Global keybindings cheat ribbon
        spans.extend(vec![
            Span::styled("[Enter] ", style_header()),
            Span::styled("Browser  ", Style::default().fg(COLOR_WHITE)),
            Span::styled("[Alt+C] ", style_header()),
            Span::styled("Copy  ", Style::default().fg(COLOR_WHITE)),
            Span::styled("[Alt+P] ", style_header()),
            Span::styled("Player  ", Style::default().fg(COLOR_WHITE)),
            Span::styled("[Alt+F] ", style_header()),
            Span::styled("Folder  ", Style::default().fg(COLOR_WHITE)),
            Span::styled("[Tab] ", style_header()),
            Span::styled("Category  ", Style::default().fg(COLOR_WHITE)),
            Span::styled("[↑/↓] ", style_header()),
            Span::styled("Navigate  ", Style::default().fg(COLOR_WHITE)),
            Span::styled("[Ctrl+U] ", style_header()),
            Span::styled("Clear  ", Style::default().fg(COLOR_WHITE)),
            Span::styled("[Esc] ", style_header()),
            Span::styled("Quit", Style::default().fg(COLOR_WHITE)),
        ]);

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}
