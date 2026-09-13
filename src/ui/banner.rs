//! Header banner widget.
//!
//! Renders a sleek, compact single-line brand header without telemetry data clutter,
//! maximizing vertical viewport space for search results and inspector details.

use crate::app::App;
use crate::ui::theme::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

/// Compact header banner widget displaying branding and application context.
pub struct BannerWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for BannerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        let header_line = Line::from(vec![
            Span::styled(" ◈ SAM-FUZZY ◈ ", style_header()),
            Span::styled(" // ", style_dim()),
            Span::styled("SAMONLINE MEDIA EXPLORER", Style::default().fg(COLOR_WHITE)),
        ]);

        Paragraph::new(header_line).render(area, buf);
    }
}
