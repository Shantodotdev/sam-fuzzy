//! Header banner and telemetry bar widget.
//!
//! Renders the top brand header in either full cyberpunk ASCII art or a compact
//! single-line format depending on terminal dimensions. Displays real-time search
//! telemetry including query latency (microseconds/milliseconds) and match statistics.

use crate::app::App;
use crate::ui::theme::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

/// Large cyberpunk ASCII logo for "SAM FUZZY".
/// Formatted cleanly without punctuation artifacts for consistent terminal rendering.
pub const BANNER_ASCII: &str = "\
  ███████╗  █████╗  ███╗   ███╗    ███████╗ ██╗   ██╗ ███████╗ ███████╗ ██╗   ██╗
  ██╔════╝ ██╔══██╗ ████╗ ████║    ██╔════╝ ██║   ██║ ╚══███╔╝ ╚══███╔╝ ╚██╗ ██╔╝
  ███████╗ ███████║ ██╔████╔██║    █████╗   ██║   ██║   ███╔╝    ███╔╝   ╚████╔╝ 
  ╚════██║ ██╔══██║ ██║╚██╔╝██║    ██╔══╝   ██║   ██║  ███╔╝    ███╔╝     ╚██╔╝  
  ███████║ ██║  ██║ ██║ ╚═╝ ██║    ██║      ╚██████╔╝ ███████╗ ███████╗    ██║   
  ╚══════╝ ╚═╝  ╚═╝ ╚═╝     ╚═╝    ╚═╝       ╚═════╝  ╚══════╝ ╚══════╝    ╚═╝   ";

/// Spaced subtitle matching the Blacksparrow aesthetic.
pub const BANNER_SPACED: &str = "  S A M   F U Z Z Y";

/// Top banner widget displaying branding, server context, and real-time search metrics.
pub struct BannerWidget<'a> {
    pub app: &'a App,
}

impl<'a> Widget for BannerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Skip rendering entirely if area is too small
        if area.height < 2 {
            return;
        }

        // Format search latency cleanly: show microseconds for sub-millisecond queries,
        // otherwise display formatted milliseconds with two decimal places.
        let micros = self.app.search_latency.as_micros();
        let latency_display = if micros < 1000 {
            format!("{}µs", micros)
        } else {
            format!("{:.2}ms", self.app.search_latency.as_secs_f64() * 1000.0)
        };

        // Render full multi-line banner if terminal height and width allow it.
        // Requires at least 8 vertical rows and 82 columns to avoid line wrapping.
        if area.height >= 8 && area.width >= 82 {
            let mut lines = Vec::new();
            for line in BANNER_ASCII.lines() {
                lines.push(Line::from(vec![
                    Span::styled(line, style_header()),
                ]));
            }

            // Subtitle with spaced typography and host description
            lines.push(Line::from(vec![
                Span::styled(BANNER_SPACED, style_border_focused()),
                Span::styled("   //   ", style_dim()),
                Span::styled("SAMONLINE (DHAKA-FLIX) FUZZY MEDIA EXPLORER", style_dim()),
            ]));

            // Real-time telemetry bar: query latency, total index size, and match count
            lines.push(Line::from(vec![
                Span::styled("  ⚡ LATENCY: ", style_dim()),
                Span::styled(latency_display, style_badge_green()),
                Span::styled("   │   ", style_dim()),
                Span::styled("📁 TOTAL: ", style_dim()),
                Span::styled(format!("{} items", self.app.engine.total_count()), style_badge_cyan()),
                Span::styled("   │   ", style_dim()),
                Span::styled("🎯 MATCHES: ", style_dim()),
                Span::styled(format!("{}", self.app.results.len()), style_badge_yellow()),
                Span::styled("   │   ", style_dim()),
                Span::styled("🌐 HOSTS: ", style_dim()),
                Span::styled("DhakaFlix (7 / 14 / 12)", style_dim()),
            ]));

            Paragraph::new(lines).render(area, buf);
        } else {
            // Compact single-line header for smaller terminal windows
            let line1 = Line::from(vec![
                Span::styled(" ◈ SAM-FUZZY ◈ ", style_header()),
                Span::styled("SAMONLINE MEDIA EXPLORER  ", style_border()),
                Span::styled("⚡ ", style_badge_green()),
                Span::styled(latency_display, style_badge_green()),
                Span::styled(" │ ", style_dim()),
                Span::styled(format!("MATCHES: {}", self.app.results.len()), style_badge_yellow()),
                Span::styled(" │ ", style_dim()),
                Span::styled(format!("TOTAL: {}", self.app.engine.total_count()), style_dim()),
            ]);

            Paragraph::new(line1).render(area, buf);
        }
    }
}
