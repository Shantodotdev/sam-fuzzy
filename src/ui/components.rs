//! UI components for search input, category tabs, result listing, inspector, and footer.
//!
//! Each widget implements Ratatui's [`Widget`] trait, drawing directly into
//! the frame buffer with zero allocations in tight render loops.

use crate::app::{App, CATEGORIES, DownloadState};
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
                Span::styled(
                    "Type to fuzzy search movies, series, years, resolutions... (e.g. 'witcher 2025', 'dark knight 1080p')",
                    style_dim(),
                ),
            ]
        } else {
            vec![
                Span::styled("❯ ", style_header()),
                Span::styled(
                    &self.app.query,
                    Style::default()
                        .fg(COLOR_WHITE)
                        .add_modifier(Modifier::BOLD),
                ),
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
        let mut spans = vec![Span::styled(" CATEGORY: ", style_dim())];

        // Render each category as a selectable pill
        for (i, &cat) in CATEGORIES.iter().enumerate() {
            if i == self.app.category_index {
                // Active tab: bold white text over maroon background
                spans.push(Span::styled(
                    format!(" ◆ {} ", cat.to_uppercase()),
                    Style::default()
                        .fg(COLOR_WHITE)
                        .bg(COLOR_MAROON)
                        .add_modifier(Modifier::BOLD),
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
            if self.app.results.is_empty() {
                0
            } else {
                self.app.selected_index + 1
            },
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
                Line::from(Span::styled(
                    "  No matching media found on SamOnline.",
                    style_dim(),
                )),
                Line::from(Span::styled(
                    "  Try clearing or simplifying your fuzzy query.",
                    style_dim(),
                )),
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

        let visible_items = self
            .app
            .results
            .iter()
            .skip(scroll_top)
            .take(visible_height);

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
                Style::default()
                    .fg(COLOR_WHITE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_WHITE)
            };

            // Character-level match highlighting
            if res.indices.is_empty() {
                let s: String = visible_chars.iter().collect();
                line_spans.push(Span::styled(s, title_style));
            } else {
                let match_set: std::collections::HashSet<u32> =
                    res.indices.iter().cloned().collect();
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
            let padding_spaces =
                inner_width.saturating_sub(prefix_char_count + visible_len + res_char_count);
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

/// Expandable bottom download manager with live transfer progress.
pub struct DownloadsWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for DownloadsWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let active = self.app.active_download_count();
        let title = format!(" DOWNLOADS  •  {active} ACTIVE  •  F7 TO HIDE ");
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(style_border_focused())
            .title(Span::styled(title, style_header()));
        let inner = block.inner(area);
        block.render(area, buf);

        if self.app.downloads.is_empty() {
            Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled("  No downloads yet.", style_dim())),
                Line::from(Span::styled(
                    "  Select a file and press Alt+D or F6 to save it here.",
                    style_dim(),
                )),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("  Target: ", style_header()),
                    Span::styled(
                        self.app.download_dir.display().to_string(),
                        style_badge_cyan(),
                    ),
                ]),
            ])
            .render(inner, buf);
            return;
        }

        let max_rows = inner.height as usize / 4;
        let start = self.app.downloads.len().saturating_sub(max_rows.max(1));
        let mut lines = Vec::new();
        for task in self.app.downloads.iter().skip(start) {
            let state = match &task.state {
                DownloadState::Queued => ("● QUEUED", style_badge_yellow()),
                DownloadState::Downloading => ("● DOWNLOADING", style_badge_cyan()),
                DownloadState::Completed => ("✓ COMPLETE", style_badge_green()),
                DownloadState::Failed(_) => (
                    "✕ FAILED",
                    Style::default().fg(COLOR_RED).add_modifier(Modifier::BOLD),
                ),
            };
            let filename = truncate_text(&task.filename, inner.width.saturating_sub(18) as usize);
            lines.push(Line::from(vec![
                Span::styled("  ", style_dim()),
                Span::styled(state.0, state.1),
                Span::styled("  ", style_dim()),
                Span::styled(
                    filename,
                    Style::default()
                        .fg(COLOR_WHITE)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            let bar_width = inner.width.saturating_sub(4) as usize;
            let fraction = task
                .total
                .filter(|total| *total > 0)
                .map(|total| task.downloaded as f64 / total as f64)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            lines.push(Line::from(Span::styled(
                format!("  {}", progress_bar(fraction, bar_width.saturating_sub(2))),
                if matches!(task.state, DownloadState::Failed(_)) {
                    Style::default().fg(COLOR_RED)
                } else {
                    style_header()
                },
            )));

            let progress = match task.total {
                Some(total) => format!(
                    "{} / {}  {:>5.1}%",
                    format_bytes(task.downloaded),
                    format_bytes(total),
                    fraction * 100.0
                ),
                None => format!(
                    "{} downloaded  •  size unknown",
                    format_bytes(task.downloaded)
                ),
            };
            let detail = match &task.state {
                DownloadState::Downloading if task.bytes_per_second > 0.0 => {
                    format!(
                        "{}  •  {}/s",
                        progress,
                        format_bytes(task.bytes_per_second as u64)
                    )
                }
                DownloadState::Failed(error) => truncate_text(error, bar_width),
                DownloadState::Completed => format!("{}  •  saved", progress),
                _ => progress,
            };
            lines.push(Line::from(Span::styled(format!("  {detail}"), style_dim())));
            lines.push(Line::raw(""));
        }
        Paragraph::new(lines).render(inner, buf);
    }
}

fn progress_bar(fraction: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let filled = (fraction * width as f64).round() as usize;
    format!(
        "[{}{}]",
        "█".repeat(filled.min(width)),
        "░".repeat(width.saturating_sub(filled))
    )
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn truncate_text(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".to_string();
    }
    format!("{}…", text.chars().take(width - 1).collect::<String>())
}

impl<'a> Widget for InspectorWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 2 || area.height < 2 {
            return;
        }

        let title = if area.width < 18 {
            " DETAILS "
        } else {
            " MEDIA DETAILS "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_border())
            .title(Span::styled(title, style_header()));

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

        // Responsive labels: adapt prefix width when inner space is constrained (< 28 cols)
        let compact = inner_area.width < 28;
        let lbl_title = if compact { "Title: " } else { " Title    : " };
        let lbl_year = if compact { "Year: " } else { " Year     : " };
        let lbl_quality = if compact { "Quality: " } else { " Quality  : " };
        let lbl_size = if compact { "Size: " } else { " Size     : " };
        let lbl_category = if compact {
            "Category: "
        } else {
            " Category : "
        };
        let lbl_server = if compact { "Server: " } else { " Server   : " };
        let lbl_file = if compact { "File: " } else { " File     : " };
        let lbl_folder = if compact { "Folder: " } else { " Folder   : " };

        // 1. Primary Title
        lines.push(Line::from(vec![
            Span::styled(lbl_title, style_header()),
            Span::styled(
                item.display_title(),
                Style::default()
                    .fg(COLOR_WHITE)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // 2. Release Year
        let year_str = item
            .year
            .map(|y| y.to_string())
            .unwrap_or_else(|| "N/A".to_string());
        lines.push(Line::from(vec![
            Span::styled(lbl_year, style_dim()),
            Span::styled(year_str, style_badge_green()),
        ]));

        // 3. Quality Profile
        lines.push(Line::from(vec![
            Span::styled(lbl_quality, style_dim()),
            Span::styled(&item.quality, style_badge_cyan()),
        ]));

        // 4. File Size (if available)
        if let Some(size) = &item.size {
            lines.push(Line::from(vec![
                Span::styled(lbl_size, style_dim()),
                Span::styled(size, style_badge_green()),
            ]));
        }

        // 5. Category
        lines.push(Line::from(vec![
            Span::styled(lbl_category, style_dim()),
            Span::styled(&item.category, style_badge_yellow()),
        ]));

        // 6. Server Mirror Host
        lines.push(Line::from(vec![
            Span::styled(lbl_server, style_dim()),
            Span::styled(&item.server, style_dim()),
        ]));

        lines.push(Line::raw(""));

        // 7. Physical Filename
        lines.push(Line::from(vec![
            Span::styled(lbl_file, style_dim()),
            Span::styled(&item.filename, Style::default().fg(COLOR_WHITE)),
        ]));

        // 8. Directory Folder Hierarchy
        if let Some((folder, _)) = item.path.rsplit_once('/') {
            let display_folder = folder
                .strip_prefix(&item.server)
                .map(|s| s.trim_start_matches('/'))
                .unwrap_or(folder);
            lines.push(Line::from(vec![
                Span::styled(lbl_folder, style_dim()),
                Span::styled(display_folder, style_dim()),
            ]));
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(inner_area, buf);
    }
}

struct ActionItem {
    key: &'static str,
    key_compact: &'static str,
    full: &'static str,
    short: &'static str,
    tiny: &'static str,
}

const ACTION_ITEMS: [ActionItem; 6] = [
    ActionItem {
        key: "[Enter] ",
        key_compact: "[↵] ",
        full: "Browser",
        short: "Browser",
        tiny: "Open",
    },
    ActionItem {
        key: "[Alt+C] ",
        key_compact: "[A-C] ",
        full: "Copy",
        short: "Copy",
        tiny: "Copy",
    },
    ActionItem {
        key: "[Alt+P] ",
        key_compact: "[A-P] ",
        full: "Player",
        short: "Play",
        tiny: "Play",
    },
    ActionItem {
        key: "[Alt+D] ",
        key_compact: "[A-D] ",
        full: "Download",
        short: "Download",
        tiny: "DL",
    },
    ActionItem {
        key: "[Alt+S] ",
        key_compact: "[A-S] ",
        full: "Sidebar",
        short: "Sidebar",
        tiny: "Side",
    },
    ActionItem {
        key: "[Alt+F] ",
        key_compact: "[A-F] ",
        full: "Folder",
        short: "Folder",
        tiny: "Dir",
    },
];

struct NavItem {
    key: &'static str,
    key_compact: &'static str,
    full: &'static str,
    short: &'static str,
    tiny: &'static str,
}

const NAV_ITEMS: [NavItem; 4] = [
    NavItem {
        key: "[Tab] ",
        key_compact: "[Tab] ",
        full: "Category",
        short: "Cat",
        tiny: "Cat",
    },
    NavItem {
        key: "[↑/↓] ",
        key_compact: "[↑/↓] ",
        full: "Navigate",
        short: "Nav",
        tiny: "Nav",
    },
    NavItem {
        key: "[Ctrl+U] ",
        key_compact: "[^U] ",
        full: "Clear",
        short: "Clear",
        tiny: "Clear",
    },
    NavItem {
        key: "[Esc] ",
        key_compact: "[Esc] ",
        full: "Quit",
        short: "Quit",
        tiny: "Quit",
    },
];

/// Renders row 1 of the footer containing all primary media action shortcuts.
pub fn render_actions_line(width: usize) -> Line<'static> {
    struct ActionTier {
        use_compact_key: bool,
        label_selector: fn(&ActionItem) -> &'static str,
        spacious: bool,
    }

    let tiers = [
        ActionTier {
            use_compact_key: false,
            label_selector: |item| item.full,
            spacious: true, // 78 cols
        },
        ActionTier {
            use_compact_key: false,
            label_selector: |item| item.full,
            spacious: false, // 74 cols
        },
        ActionTier {
            use_compact_key: false,
            label_selector: |item| item.short,
            spacious: false, // 72 cols
        },
        ActionTier {
            use_compact_key: false,
            label_selector: |item| item.tiny,
            spacious: false, // 63 cols
        },
        ActionTier {
            use_compact_key: false,
            label_selector: |item| item.tiny,
            spacious: false,
        },
        ActionTier {
            use_compact_key: true,
            label_selector: |item| item.tiny,
            spacious: false, // 51 cols
        },
    ];

    let chosen = tiers
        .iter()
        .find(|t| {
            let spacing = if t.spacious { 2 } else { 1 };
            let mut w = 0;
            for (i, item) in ACTION_ITEMS.iter().enumerate() {
                let key = if t.use_compact_key {
                    item.key_compact
                } else {
                    item.key
                };
                let label = (t.label_selector)(item);
                w += key.chars().count() + label.chars().count();
                if i + 1 < ACTION_ITEMS.len() {
                    w += spacing;
                }
            }
            w <= width
        })
        .unwrap_or(&tiers[tiers.len() - 1]);

    let sep = if chosen.spacious { "  " } else { " " };
    let mut spans = Vec::with_capacity(ACTION_ITEMS.len() * 3);
    for (i, item) in ACTION_ITEMS.iter().enumerate() {
        let key = if chosen.use_compact_key {
            item.key_compact
        } else {
            item.key
        };
        let label = (chosen.label_selector)(item);
        spans.push(Span::styled(key, style_header()));
        spans.push(Span::styled(label, Style::default().fg(COLOR_WHITE)));
        if i + 1 < ACTION_ITEMS.len() {
            spans.push(Span::styled(sep, Style::default().fg(COLOR_WHITE)));
        }
    }

    Line::from(spans)
}

/// Renders row 2 of the footer containing navigation, clear, quit, and transient status notifications.
pub fn render_nav_line(width: usize, status: Option<(&str, bool)>) -> Line<'static> {
    let mut spans = Vec::new();
    let mut status_width = 0;

    if let Some((status_msg, is_err)) = status {
        let status_style = if is_err {
            Style::default().fg(COLOR_RED).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(COLOR_NEON_GREEN)
                .add_modifier(Modifier::BOLD)
        };
        let formatted = format!(" {} ", status_msg);
        status_width = formatted.chars().count() + 3; // + " │ "
        spans.push(Span::styled(formatted, status_style));
        spans.push(Span::styled(" │ ", style_dim()));
    }

    let rem_width = width.saturating_sub(status_width);

    struct NavTier {
        use_compact_key: bool,
        label_selector: fn(&NavItem) -> &'static str,
        spacious: bool,
    }

    let tiers = [
        NavTier {
            use_compact_key: false,
            label_selector: |item| item.full,
            spacious: true, // 58 cols
        },
        NavTier {
            use_compact_key: false,
            label_selector: |item| item.full,
            spacious: false, // 55 cols
        },
        NavTier {
            use_compact_key: false,
            label_selector: |item| item.short,
            spacious: false, // 45 cols
        },
        NavTier {
            use_compact_key: true,
            label_selector: |item| item.tiny,
            spacious: false, // 41 cols
        },
    ];

    let chosen = tiers
        .iter()
        .find(|t| {
            let spacing = if t.spacious { 2 } else { 1 };
            let mut w = 0;
            for (i, item) in NAV_ITEMS.iter().enumerate() {
                let key = if t.use_compact_key {
                    item.key_compact
                } else {
                    item.key
                };
                let label = (t.label_selector)(item);
                w += key.chars().count() + label.chars().count();
                if i + 1 < NAV_ITEMS.len() {
                    w += spacing;
                }
            }
            w <= rem_width
        })
        .unwrap_or(&tiers[tiers.len() - 1]);

    let sep = if chosen.spacious { "  " } else { " " };
    for (i, item) in NAV_ITEMS.iter().enumerate() {
        let key = if chosen.use_compact_key {
            item.key_compact
        } else {
            item.key
        };
        let label = (chosen.label_selector)(item);
        spans.push(Span::styled(key, style_header()));
        spans.push(Span::styled(label, Style::default().fg(COLOR_WHITE)));
        if i + 1 < NAV_ITEMS.len() {
            spans.push(Span::styled(sep, Style::default().fg(COLOR_WHITE)));
        }
    }

    Line::from(spans)
}

struct SingleItem {
    key: &'static str,
    key_compact: &'static str,
    full: &'static str,
    short: &'static str,
    tiny: &'static str,
}

const ALL_9_ITEMS: [SingleItem; 10] = [
    SingleItem {
        key: "[Enter] ",
        key_compact: "[Enter] ",
        full: "Browser",
        short: "Browser",
        tiny: "Open",
    },
    SingleItem {
        key: "[Alt+C] ",
        key_compact: "[Alt+C] ",
        full: "Copy",
        short: "Copy",
        tiny: "Copy",
    },
    SingleItem {
        key: "[Alt+P] ",
        key_compact: "[Alt+P] ",
        full: "Player",
        short: "Play",
        tiny: "Play",
    },
    SingleItem {
        key: "[Alt+D] ",
        key_compact: "[A-D] ",
        full: "Download",
        short: "Download",
        tiny: "DL",
    },
    SingleItem {
        key: "[Alt+S] ",
        key_compact: "[Alt+S] ",
        full: "Sidebar",
        short: "Sidebar",
        tiny: "Side",
    },
    SingleItem {
        key: "[Alt+F] ",
        key_compact: "[Alt+F] ",
        full: "Folder",
        short: "Folder",
        tiny: "Dir",
    },
    SingleItem {
        key: "[Tab] ",
        key_compact: "[Tab] ",
        full: "Category",
        short: "Cat",
        tiny: "Cat",
    },
    SingleItem {
        key: "[↑/↓] ",
        key_compact: "[↑/↓] ",
        full: "Navigate",
        short: "Nav",
        tiny: "Nav",
    },
    SingleItem {
        key: "[Ctrl+U] ",
        key_compact: "[^U] ",
        full: "Clear",
        short: "Clear",
        tiny: "Clear",
    },
    SingleItem {
        key: "[Esc] ",
        key_compact: "[Esc] ",
        full: "Quit",
        short: "Quit",
        tiny: "Quit",
    },
];

/// Renders all shortcuts in a single responsive row when large width is available.
pub fn render_single_line(width: usize, status: Option<(&str, bool)>) -> Line<'static> {
    let mut spans = Vec::new();
    let mut status_width = 0;

    if let Some((status_msg, is_err)) = status {
        let status_style = if is_err {
            Style::default().fg(COLOR_RED).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(COLOR_NEON_GREEN)
                .add_modifier(Modifier::BOLD)
        };
        let formatted = format!(" {} ", status_msg);
        status_width = formatted.chars().count() + 3; // + " │ "
        spans.push(Span::styled(formatted, status_style));
        spans.push(Span::styled(" │ ", style_dim()));
    }

    let rem_width = width.saturating_sub(status_width);

    struct SingleTier {
        use_compact_key: bool,
        label_selector: fn(&SingleItem) -> &'static str,
        spacious: bool,
    }

    let tiers = [
        SingleTier {
            use_compact_key: false,
            label_selector: |item| item.full,
            spacious: true, // 128 cols
        },
        SingleTier {
            use_compact_key: false,
            label_selector: |item| item.full,
            spacious: false, // 120 cols
        },
        SingleTier {
            use_compact_key: false,
            label_selector: |item| item.short,
            spacious: false, // 108 cols
        },
        SingleTier {
            use_compact_key: true,
            label_selector: |item| item.tiny,
            spacious: false, // 101 cols
        },
    ];

    let chosen = tiers
        .iter()
        .find(|t| {
            let spacing = if t.spacious { 2 } else { 1 };
            let mut w = 0;
            for (i, item) in ALL_9_ITEMS.iter().enumerate() {
                let key = if t.use_compact_key {
                    item.key_compact
                } else {
                    item.key
                };
                let label = (t.label_selector)(item);
                w += key.chars().count() + label.chars().count();
                if i + 1 < ALL_9_ITEMS.len() {
                    w += spacing;
                }
            }
            w <= rem_width
        })
        .unwrap_or(&tiers[tiers.len() - 1]);

    let sep = if chosen.spacious { "  " } else { " " };
    for (i, item) in ALL_9_ITEMS.iter().enumerate() {
        let key = if chosen.use_compact_key {
            item.key_compact
        } else {
            item.key
        };
        let label = (chosen.label_selector)(item);
        spans.push(Span::styled(key, style_header()));
        spans.push(Span::styled(label, Style::default().fg(COLOR_WHITE)));
        if i + 1 < ALL_9_ITEMS.len() {
            spans.push(Span::styled(sep, Style::default().fg(COLOR_WHITE)));
        }
    }

    Line::from(spans)
}

/// Bottom status line displaying all keybindings alongside toast notifications.
///
/// Responsively renders in:
/// - One row when there is enough space (width >= 120 columns).
/// - Two rows on narrower terminals (< 120 columns) to prevent any cutoff.
pub struct FooterWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for FooterWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 5 || area.height < 1 {
            return;
        }

        let width = area.width as usize;
        let status = self.app.active_status();

        let lines = if area.height >= 2 {
            let line1 = render_actions_line(width);
            let line2 = render_nav_line(width, status);
            vec![line1, line2]
        } else {
            vec![render_single_line(width, status)]
        };

        Paragraph::new(lines).render(area, buf);
    }
}
