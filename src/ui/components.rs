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
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap};

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

        let inner_width = inner_area.width as usize;
        let idx_width = self.app.results.len().to_string().len();

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

            // 1-based index number right-aligned to match the maximum digit width of results,
            // formatted without dot and followed by a single space so all filenames start
            // at the exact same horizontal alignment column.
            let index_str = format!("{:>width$} ", current_idx + 1, width = idx_width);
            let prefix_char_count = 3 + index_str.chars().count();
            line_spans.push(Span::styled(index_str, style_index(is_selected)));

            // Resolution tag (e.g. 1080p, 720p, 4K) right-aligned at the far right edge of the list pane
            let res_tag = res.item.clean_resolution();
            let (res_str, res_char_count) = if let Some(r) = res_tag {
                (format!("{:>5} ", r), 6)
            } else {
                (String::new(), 0)
            };

            // Calculate available columns for the title
            let available_title = inner_width.saturating_sub(prefix_char_count + res_char_count);
            let display_title = res.item.display_title();
            let title_chars: Vec<char> = display_title.chars().collect();
            let title_len = title_chars.len();

            let (visible_chars, visible_len, is_truncated) = if title_len > available_title {
                let take_count = available_title.saturating_sub(1);
                (&title_chars[..take_count], take_count + 1, true)
            } else {
                (&title_chars[..], title_len, false)
            };

            let title_style = if is_selected {
                Style::default().fg(COLOR_WHITE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_WHITE)
            };

            // Character-level match highlighting
            if res.indices.is_empty() {
                let s: String = visible_chars.iter().collect();
                line_spans.push(Span::styled(s, title_style));
            } else {
                let match_set: std::collections::HashSet<u32> = res.indices.iter().cloned().collect();
                for (char_idx, &ch) in visible_chars.iter().enumerate() {
                    if match_set.contains(&(char_idx as u32)) {
                        line_spans.push(Span::styled(ch.to_string(), style_highlight_match()));
                    } else {
                        line_spans.push(Span::styled(ch.to_string(), title_style));
                    }
                }
            }

            if is_truncated {
                line_spans.push(Span::styled("…", style_dim()));
            }

            // Fill padding spaces to push the resolution tag flush against the right side
            let padding_spaces = inner_width.saturating_sub(prefix_char_count + visible_len + res_char_count);
            if padding_spaces > 0 {
                line_spans.push(Span::styled(" ".repeat(padding_spaces), title_style));
            }

            // Right-aligned resolution tag
            if !res_str.is_empty() {
                line_spans.push(Span::styled(res_str, style_badge_cyan()));
            }

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

/// Compact metadata inspector panel for the currently selected media item.
///
/// Displays core movie/series attributes:
/// - Clean display title, release year, and encoding quality profile.
/// - Section category and originating mirror server ID.
/// - Physical filename and server folder location.
pub struct InspectorWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for InspectorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_border())
            .title(Span::styled(" MEDIA DETAILS ", style_header()));

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
            Span::styled(" Title    : ", style_header()),
            Span::styled(
                item.display_title(),
                Style::default().fg(COLOR_WHITE).add_modifier(Modifier::BOLD),
            ),
        ]));

        // 2. Release Year
        let year_str = item.year.map(|y| y.to_string()).unwrap_or_else(|| "N/A".to_string());
        lines.push(Line::from(vec![
            Span::styled(" Year     : ", style_dim()),
            Span::styled(year_str, style_badge_green()),
        ]));

        // 3. Quality Profile
        lines.push(Line::from(vec![
            Span::styled(" Quality  : ", style_dim()),
            Span::styled(&item.quality, style_badge_cyan()),
        ]));

        // 4. File Size (if available)
        if let Some(size) = &item.size {
            lines.push(Line::from(vec![
                Span::styled(" Size     : ", style_dim()),
                Span::styled(size, style_badge_green()),
            ]));
        }

        // 5. Category
        lines.push(Line::from(vec![
            Span::styled(" Category : ", style_dim()),
            Span::styled(&item.category, style_badge_yellow()),
        ]));

        // 5. Server Mirror Host
        lines.push(Line::from(vec![
            Span::styled(" Server   : ", style_dim()),
            Span::styled(&item.server, style_dim()),
        ]));

        lines.push(Line::raw(""));

        // 6. Physical Filename
        lines.push(Line::from(vec![
            Span::styled(" File     : ", style_dim()),
            Span::styled(&item.filename, Style::default().fg(COLOR_WHITE)),
        ]));

        // 7. Directory Folder Hierarchy
        if let Some((folder, _)) = item.path.rsplit_once('/') {
            let display_folder = folder
                .strip_prefix(&item.server)
                .map(|s| s.trim_start_matches('/'))
                .unwrap_or(folder);
            lines.push(Line::from(vec![
                Span::styled(" Folder   : ", style_dim()),
                Span::styled(display_folder, style_dim()),
            ]));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(inner_area, buf);
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
