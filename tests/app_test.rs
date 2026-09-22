use sam_fuzzy::app::App;
use sam_fuzzy::fuzzy::SearchEngine;
use sam_fuzzy::models::MediaItem;

fn sample_items() -> Vec<MediaItem> {
    vec![
        MediaItem {
            id: 1,
            title: "Batman Begins".to_string(),
            year: Some(2005),
            quality: "1080p".to_string(),
            category: "English Movies".to_string(),
            filename: "Batman.Begins.2005.1080p.mkv".to_string(),
            is_file: true,
            url: "http://172.16.50.7/batman.mkv".to_string(),
            folder_url: "http://172.16.50.7/batman/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/English Movies/batman.mkv".to_string(),
            size: Some("2.10 GB".to_string()),
        },
        MediaItem {
            id: 2,
            title: "The Prestige".to_string(),
            year: Some(2006),
            quality: "1080p".to_string(),
            category: "English Movies".to_string(),
            filename: "The.Prestige.2006.1080p.mkv".to_string(),
            is_file: true,
            url: "http://172.16.50.7/prestige.mkv".to_string(),
            folder_url: "http://172.16.50.7/prestige/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/English Movies/prestige.mkv".to_string(),
            size: Some("1.85 GB".to_string()),
        },
        MediaItem {
            id: 3,
            title: "Oldboy".to_string(),
            year: Some(2003),
            quality: "1080p".to_string(),
            category: "Foreign Language Movies / Korean Language".to_string(),
            filename: "Oldboy.2003.1080p.mkv".to_string(),
            is_file: true,
            url: "http://172.16.50.7/oldboy.mkv".to_string(),
            folder_url: "http://172.16.50.7/oldboy/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/Foreign Language Movies/Korean Language/oldboy.mkv".to_string(),
            size: Some("1.40 GB".to_string()),
        },
    ]
}

#[test]
fn test_app_initialization_and_search() {
    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);

    assert_eq!(app.results.len(), 3);
    assert_eq!(app.selected_index, 0);
    assert_eq!(app.query, "");

    // Type "bat"
    app.on_key_char('b');
    app.on_key_char('a');
    app.on_key_char('t');

    assert_eq!(app.query, "bat");
    assert_eq!(app.results.len(), 1);
    assert_eq!(app.selected_item().unwrap().title, "Batman Begins");
}

#[test]
fn test_app_navigation_bounds() {
    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);

    assert_eq!(app.selected_index, 0);
    app.select_prev();
    assert_eq!(app.selected_index, 0); // Can't go below 0

    app.select_next();
    assert_eq!(app.selected_index, 1);
    app.select_next();
    assert_eq!(app.selected_index, 2);
    app.select_next();
    assert_eq!(app.selected_index, 2); // Can't go beyond results length - 1
}

#[test]
fn test_app_category_switching() {
    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);

    // Initial category is "All"
    assert_eq!(app.current_category(), "All");
    assert_eq!(app.results.len(), 3);

    // Switch to next category ("English")
    app.next_category();
    assert_eq!(app.current_category(), "English");
    assert_eq!(app.results.len(), 2);

    // Switch to Korean category directly
    app.set_category_by_name("Korean");
    assert_eq!(app.results.len(), 1);
    assert_eq!(app.selected_item().unwrap().title, "Oldboy");
}

#[test]
fn test_app_backspace_and_clear() {
    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);

    app.on_key_char('p');
    app.on_key_char('r');
    app.on_key_char('e');
    assert_eq!(app.query, "pre");

    app.on_backspace();
    assert_eq!(app.query, "pr");

    app.on_clear_query();
    assert_eq!(app.query, "");
    assert_eq!(app.results.len(), 3);
}

#[test]
fn test_result_list_renders_index_numbers_without_dot() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use sam_fuzzy::ui::components::ResultListWidget;

    let engine = SearchEngine::new(sample_items());
    let app = App::new(engine);

    let area = Rect::new(0, 0, 80, 6);
    let mut buf = Buffer::empty(area);
    let widget = ResultListWidget { app: &app };
    widget.render(area, &mut buf);

    let mut row_strings = Vec::new();
    for y in 0..area.height {
        let mut row = String::new();
        for x in 0..area.width {
            if let Some(cell) = buf.cell((x, y)) {
                row.push_str(cell.symbol());
            }
        }
        row_strings.push(row);
    }

    // Row contains "1 Batman Begins" (or similar), ensuring no dot is used after the index number
    assert!(
        row_strings
            .iter()
            .any(|r| r.contains("1 ") && !r.contains("1. "))
    );
    assert!(
        row_strings
            .iter()
            .any(|r| r.contains("2 ") && !r.contains("2. "))
    );
}

#[test]
fn test_result_list_aligns_single_and_double_digit_filenames() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use sam_fuzzy::ui::components::ResultListWidget;

    // Create 15 items so we have both single-digit (1..9) and double-digit (10..15) indices
    let items: Vec<MediaItem> = (1..=15)
        .map(|i| MediaItem {
            id: i,
            title: format!("Movie Episode {:02}", i),
            year: Some(2025),
            quality: "1080p".to_string(),
            category: "TV Series".to_string(),
            filename: format!("Movie.E{:02}.mkv", i),
            is_file: true,
            url: format!("http://172.16.50.14/m{:02}.mkv", i),
            folder_url: "http://172.16.50.14/".to_string(),
            server: "DHAKA-FLIX-14".to_string(),
            path: format!("DHAKA-FLIX-14/m{:02}.mkv", i),
            size: Some(format!("{}00 MB", i)),
        })
        .collect();

    let engine = SearchEngine::new(items);
    let app = App::new(engine);

    let area = Rect::new(0, 0, 80, 16);
    let mut buf = Buffer::empty(area);
    let widget = ResultListWidget { app: &app };
    widget.render(area, &mut buf);

    let mut row_strings = Vec::new();
    for y in 0..area.height {
        let mut row = String::new();
        for x in 0..area.width {
            if let Some(cell) = buf.cell((x, y)) {
                row.push_str(cell.symbol());
            }
        }
        row_strings.push(row);
    }

    // Find row with "Movie Episode 02" (index 2) and row with "Movie Episode 10" (index 10)
    let row_single = row_strings
        .iter()
        .find(|r| r.contains("Movie Episode 02"))
        .unwrap();
    let row_double = row_strings
        .iter()
        .find(|r| r.contains("Movie Episode 10"))
        .unwrap();

    let col_single = row_single.find("Movie Episode 02").unwrap();
    let col_double = row_double.find("Movie Episode 10").unwrap();

    assert_eq!(
        col_single, col_double,
        "Single-digit and double-digit items must start at the exact same column ({col_single} vs {col_double})"
    );
}

#[test]
fn test_result_list_renders_right_aligned_resolution_only() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use sam_fuzzy::ui::components::ResultListWidget;

    let engine = SearchEngine::new(sample_items());
    let app = App::new(engine);

    let area = Rect::new(0, 0, 70, 5);
    let mut buf = Buffer::empty(area);
    let widget = ResultListWidget { app: &app };
    widget.render(area, &mut buf);

    let mut row_strings = Vec::new();
    for y in 0..area.height {
        let mut row = String::new();
        for x in 0..area.width {
            if let Some(cell) = buf.cell((x, y)) {
                row.push_str(cell.symbol());
            }
        }
        row_strings.push(row);
    }

    // Check row 0 (Batman Begins, 1080p)
    let batman_row = row_strings
        .iter()
        .find(|r| r.contains("Batman Begins"))
        .unwrap();
    // Must contain 1080p at the far right
    assert!(batman_row.ends_with("1080p ") || batman_row.contains("1080p"));
    // Must NOT contain size badges or category tags
    assert!(!batman_row.contains("2.10 GB"));
    assert!(!batman_row.contains("English Movies"));
    assert!(!batman_row.contains("•"));
}

#[test]
fn test_inspector_widget_simplified_rendering() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use sam_fuzzy::ui::components::InspectorWidget;

    let engine = SearchEngine::new(sample_items());
    let app = App::new(engine);

    let area = Rect::new(0, 0, 50, 20);
    let mut buf = Buffer::empty(area);
    let widget = InspectorWidget { app: &app };
    widget.render(area, &mut buf);

    let mut content = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            if let Some(cell) = buf.cell((x, y)) {
                content.push_str(cell.symbol());
            }
        }
        content.push('\n');
    }

    // Must have clean simplified title and essential metadata labels
    assert!(content.contains("MEDIA DETAILS"));
    assert!(content.contains("Title    : Batman Begins"));
    assert!(content.contains("Year     : 2005"));
    assert!(content.contains("Quality  : 1080p"));
    assert!(content.contains("Size     : 2.10 GB"));
    assert!(content.contains("Category : English Movies"));
    assert!(content.contains("Server   : DHAKA-FLIX-7"));
    assert!(content.contains("File     : Batman.Begins.2005.1080p.mkv"));
    assert!(content.contains("Folder   : English Movies"));

    // Must NOT contain removed clutter
    assert!(!content.contains("DIRECT STREAM URL"));
    assert!(!content.contains("PARENT FOLDER URL"));
    assert!(!content.contains("ACTION SHORTCUTS"));
    assert!(!content.contains("PATH BREADCRUMBS"));
    assert!(!content.contains("Direct Video File"));
}

#[test]
fn test_banner_widget_simplified_rendering() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use sam_fuzzy::ui::banner::BannerWidget;

    let engine = SearchEngine::new(sample_items());
    let app = App::new(engine);

    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    let widget = BannerWidget { app: &app };
    widget.render(area, &mut buf);

    let mut content = String::new();
    for x in 0..area.width {
        if let Some(cell) = buf.cell((x, 0)) {
            content.push_str(cell.symbol());
        }
    }

    // Must have clean branding
    assert!(content.contains("SAM-FUZZY"));
    assert!(content.contains("SAMONLINE MEDIA EXPLORER"));

    // Must NOT contain removed telemetry data
    assert!(!content.contains("LATENCY"));
    assert!(!content.contains("TOTAL:"));
    assert!(!content.contains("MATCHES:"));
    assert!(!content.contains("HOSTS:"));
}

#[test]
fn test_background_search_worker() {
    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);

    app.spawn_search_worker();

    // Type a search query asynchronously
    app.on_key_char('b');
    app.on_key_char('a');
    app.on_key_char('t');

    // Poll until the worker returns results
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        app.poll_search_results();
        if app.results.len() == 1 {
            break;
        }
    }

    assert_eq!(app.query, "bat");
    assert_eq!(app.results.len(), 1);
    assert_eq!(app.results[0].item.title, "Batman Begins");
}

#[test]
fn test_sidebar_visibility_and_toggle() {
    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);

    // Initial default: None (responsive based on terminal width)
    assert_eq!(app.show_sidebar, None);
    assert!(!app.is_sidebar_visible(80)); // Narrow screen (< 100): hidden
    assert!(app.is_sidebar_visible(100)); // Wide screen (>= 100): visible
    assert!(app.is_sidebar_visible(120));

    // Toggle on narrow screen (80 cols) -> opens sidebar
    app.toggle_sidebar_with_width(80);
    assert_eq!(app.show_sidebar, Some(true));
    assert!(app.is_sidebar_visible(80));
    assert!(app.is_sidebar_visible(60));

    // Toggle again on narrow screen -> closes sidebar
    app.toggle_sidebar_with_width(80);
    assert_eq!(app.show_sidebar, Some(false));
    assert!(!app.is_sidebar_visible(80));
    assert!(!app.is_sidebar_visible(120));

    // Toggle on wide screen (120 cols) when false -> opens sidebar
    app.toggle_sidebar_with_width(120);
    assert_eq!(app.show_sidebar, Some(true));
    assert!(app.is_sidebar_visible(120));

    // Reset to None (auto)
    app.show_sidebar = None;

    // Toggle on wide screen (120 cols) when auto -> closes sidebar
    app.toggle_sidebar_with_width(120);
    assert_eq!(app.show_sidebar, Some(false));
    assert!(!app.is_sidebar_visible(120));
}

#[test]
fn test_render_ui_responsive_sidebar_toggle() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use sam_fuzzy::ui::render_ui;

    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);

    // 1. Narrow terminal (80 cols) - default sidebar closed
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render_ui(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| buffer.cell((x, y)).unwrap().symbol()))
        .collect();

    assert!(content.contains("Batman Begins"));
    assert!(!content.contains("MEDIA DETAILS"));

    // 2. Toggle sidebar open on narrow terminal (80 cols)
    app.toggle_sidebar_with_width(80);
    terminal.draw(|f| render_ui(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| buffer.cell((x, y)).unwrap().symbol()))
        .collect();

    assert!(content.contains("Batman Begins"));
    assert!(content.contains("MEDIA DETAILS"));

    // 3. Toggle sidebar closed again on narrow terminal
    app.toggle_sidebar_with_width(80);
    terminal.draw(|f| render_ui(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| buffer.cell((x, y)).unwrap().symbol()))
        .collect();

    assert!(content.contains("Batman Begins"));
    assert!(!content.contains("MEDIA DETAILS"));

    // 4. Wide terminal (120 cols) - default sidebar open
    app.show_sidebar = None;
    let backend_wide = TestBackend::new(120, 24);
    let mut terminal_wide = Terminal::new(backend_wide).unwrap();
    terminal_wide.draw(|f| render_ui(f, &app)).unwrap();
    let buffer = terminal_wide.backend().buffer();
    let content: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| buffer.cell((x, y)).unwrap().symbol()))
        .collect();

    assert!(content.contains("Batman Begins"));
    assert!(content.contains("MEDIA DETAILS"));

    // 5. Toggle sidebar closed on wide terminal
    app.toggle_sidebar_with_width(120);
    terminal_wide.draw(|f| render_ui(f, &app)).unwrap();
    let buffer = terminal_wide.backend().buffer();
    let content: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| buffer.cell((x, y)).unwrap().symbol()))
        .collect();

    assert!(content.contains("Batman Begins"));
    assert!(!content.contains("MEDIA DETAILS"));
}

#[test]
fn test_downloads_panel_renders_below_search_without_covering_it() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use sam_fuzzy::ui::render_ui;

    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);
    app.show_downloads = true;

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render_ui(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| buffer.cell((x, y)).unwrap().symbol()))
        .collect();

    assert!(content.contains("DOWNLOADS"));
    assert!(content.contains("No downloads yet."));
    assert!(content.contains("Alt+D or F6"));
    assert!(content.contains("FUZZY SEARCH"));
    assert!(!content.contains("FOCUSED"));
}

#[test]
fn test_downloads_panel_grows_for_additional_tasks_until_its_layout_budget() {
    use sam_fuzzy::app::DownloadTask;
    use std::path::PathBuf;

    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);
    app.show_downloads = true;

    assert_eq!(app.downloads_panel_height(20), 6);

    app.downloads.push(DownloadTask::queued(
        1,
        "First.mkv".to_string(),
        PathBuf::from("/tmp/First.mkv"),
    ));
    assert_eq!(app.downloads_panel_height(20), 6);

    app.downloads.push(DownloadTask::queued(
        2,
        "Second.mkv".to_string(),
        PathBuf::from("/tmp/Second.mkv"),
    ));
    assert_eq!(app.downloads_panel_height(20), 10);
    assert_eq!(app.downloads_panel_height(8), 8);
}

#[test]
fn test_footer_shows_mode_aware_download_controls_below_the_downloads_panel() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use sam_fuzzy::app::DownloadTask;
    use sam_fuzzy::ui::render_ui;
    use std::path::PathBuf;

    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);
    app.show_downloads = true;
    app.downloads.push(DownloadTask::queued(
        1,
        "Download.mkv".to_string(),
        PathBuf::from("/tmp/Download.mkv"),
    ));

    let backend = TestBackend::new(140, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render_ui(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| buffer.cell((x, y)).unwrap().symbol()))
        .collect();

    assert!(content.contains("SEARCH"));
    assert!(content.contains("[Alt+D/F6] Download selected"));
    assert!(content.contains("[F7] Show panel"));
    assert!(content.contains("[Ctrl+W] Focus downloads"));
    assert!(content.contains("[Alt+J/K] Navigate results"));

    let bottom_row: String = (0..buffer.area.width)
        .map(|x| buffer.cell((x, buffer.area.height - 1)).unwrap().symbol())
        .collect();
    assert!(bottom_row.contains("Alt+D/F6"));

    app.toggle_active_pane();
    terminal.draw(|frame| render_ui(frame, &app)).unwrap();
    let focused_buffer = terminal.backend().buffer();
    let focused_content: String = (0..focused_buffer.area.height)
        .flat_map(|y| {
            (0..focused_buffer.area.width)
                .map(move |x| focused_buffer.cell((x, y)).unwrap().symbol())
        })
        .collect();
    assert!(focused_content.contains("DOWNLOADS"));
    assert!(focused_content.contains("[Alt+C/F8] Cancel selected"));
}

#[test]
fn test_download_notification_is_rendered_below_download_shortcuts() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use sam_fuzzy::app::DownloadTask;
    use sam_fuzzy::ui::render_ui;
    use std::path::PathBuf;

    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);
    app.show_downloads = true;
    app.downloads.push(DownloadTask::queued(
        1,
        "Download.mkv".to_string(),
        PathBuf::from("/tmp/Download.mkv"),
    ));
    app.toggle_active_pane();
    app.set_status("Download started: Download.mkv", false);

    let backend = TestBackend::new(140, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render_ui(frame, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let shortcut_row: String = (0..buffer.area.width)
        .map(|x| buffer.cell((x, buffer.area.height - 2)).unwrap().symbol())
        .collect();
    let notification_row: String = (0..buffer.area.width)
        .map(|x| buffer.cell((x, buffer.area.height - 1)).unwrap().symbol())
        .collect();

    assert!(shortcut_row.contains("[Alt+C/F8] Cancel selected"));
    assert!(notification_row.contains("NOTICE"));
    assert!(notification_row.contains("Download started: Download.mkv"));
}

#[test]
fn test_selected_download_can_be_cancelled_without_affecting_other_tasks() {
    use sam_fuzzy::app::{DownloadState, DownloadTask};
    use std::path::PathBuf;

    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);
    app.downloads.push(DownloadTask::queued(
        1,
        "First.mkv".to_string(),
        PathBuf::from("/tmp/First.mkv"),
    ));
    app.downloads.push(DownloadTask::queued(
        2,
        "Second.mkv".to_string(),
        PathBuf::from("/tmp/Second.mkv"),
    ));
    app.selected_download_index = 1;

    let outcome = app.cancel_selected_download().unwrap();

    assert!(
        outcome
            .message()
            .contains("Cancelling download: Second.mkv")
    );
    assert_eq!(app.downloads[0].state, DownloadState::Queued);
    assert_eq!(app.downloads[1].state, DownloadState::Cancelling);
    assert_eq!(app.active_download_count(), 2);
}

#[test]
fn test_focus_switches_between_search_and_downloads_only_when_available() {
    use sam_fuzzy::app::{ActivePane, DownloadTask};
    use std::path::PathBuf;

    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);

    // A hidden or empty panel cannot take focus.
    app.toggle_active_pane();
    assert_eq!(app.active_pane, ActivePane::Search);

    app.show_downloads = true;
    app.downloads.push(DownloadTask::queued(
        1,
        "Download.mkv".to_string(),
        PathBuf::from("/tmp/Download.mkv"),
    ));

    app.toggle_active_pane();
    assert_eq!(app.active_pane, ActivePane::Downloads);
    assert!(app.is_downloads_focused());

    app.toggle_active_pane();
    assert_eq!(app.active_pane, ActivePane::Search);
    assert!(app.is_search_focused());
}

#[test]
fn test_inspector_widget_compact_rendering_on_very_narrow_area() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use sam_fuzzy::ui::components::InspectorWidget;

    let engine = SearchEngine::new(sample_items());
    let app = App::new(engine);

    // Area width 22 -> inner_area width 20 (< 28 cols triggers compact labels)
    let area = Rect::new(0, 0, 22, 15);
    let mut buf = Buffer::empty(area);
    let widget = InspectorWidget { app: &app };
    widget.render(area, &mut buf);

    let mut content = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            if let Some(cell) = buf.cell((x, y)) {
                content.push_str(cell.symbol());
            }
        }
        content.push('\n');
    }

    // Compact labels used when width is constrained
    assert!(content.contains("Title:"));
    assert!(content.contains("Year:"));
    assert!(content.contains("Quality:"));
    assert!(content.contains("Category:"));
}

#[test]
fn test_footer_widget_two_row_layout_preserves_all_shortcuts() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use sam_fuzzy::ui::components::{FooterWidget, render_actions_line, render_nav_line};

    let engine = SearchEngine::new(sample_items());
    let mut app = App::new(engine);

    // 1. Actions line on 80 columns contains all 5 primary action shortcuts
    let line1_80 = render_actions_line(80);
    let text_actions: String = line1_80.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text_actions.chars().count() <= 80);
    assert!(text_actions.contains("[Enter]"));
    assert!(text_actions.contains("[Alt+C]"));
    assert!(text_actions.contains("[Alt+P]"));
    assert!(text_actions.contains("[Alt+S]"));
    assert!(text_actions.contains("[Alt+F]"));

    // 2. Navigation line on 80 columns contains all 4 nav/control shortcuts
    let line2_80 = render_nav_line(80, None);
    let text_nav: String = line2_80.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text_nav.chars().count() <= 80);
    assert!(text_nav.contains("[Tab]"));
    assert!(text_nav.contains("[↑/↓]"));
    assert!(text_nav.contains("[Ctrl+U]"));
    assert!(text_nav.contains("[Esc]"));

    // 3. Render FooterWidget on an 80x2 buffer (2 rows)
    let area_80 = Rect::new(0, 0, 80, 2);
    let mut buf_80 = Buffer::empty(area_80);
    let widget_80 = FooterWidget { app: &app };
    widget_80.render(area_80, &mut buf_80);

    let row0: String = (0..area_80.width)
        .map(|x| buf_80.cell((x, 0)).unwrap().symbol().to_string())
        .collect();
    let row1: String = (0..area_80.width)
        .map(|x| buf_80.cell((x, 1)).unwrap().symbol().to_string())
        .collect();

    // Row 0 has all media actions
    assert!(row0.contains("[Enter]"));
    assert!(row0.contains("[Alt+C]"));
    assert!(row0.contains("[Alt+P]"));
    assert!(row0.contains("[Alt+S]"));
    assert!(row0.contains("[Alt+F]"));

    // Row 1 has all navigation & quit
    assert!(row1.contains("[Tab]"));
    assert!(row1.contains("[↑/↓]"));
    assert!(row1.contains("[Ctrl+U]"));
    assert!(row1.contains("[Esc]"));

    // 4. Active status toast notification on row 1 preserves shortcuts without overflow
    app.set_status("Link copied to clipboard!", false);
    let mut buf_status = Buffer::empty(area_80);
    let widget_status = FooterWidget { app: &app };
    widget_status.render(area_80, &mut buf_status);

    let row1_status: String = (0..area_80.width)
        .map(|x| buf_status.cell((x, 1)).unwrap().symbol().to_string())
        .collect();
    assert!(row1_status.contains("Link copied to clipboard!"));
    assert!(row1_status.contains("[Esc]"));
    assert!(row1_status.contains("[Tab]"));

    // 5. On large width (>= 120 columns), all 9 shortcuts are displayed in a single row
    use sam_fuzzy::ui::components::render_single_line;
    let line_single = render_single_line(140, None);
    let text_single: String = line_single
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(text_single.contains("[Enter]"));
    assert!(text_single.contains("[Alt+C]"));
    assert!(text_single.contains("[Alt+P]"));
    assert!(text_single.contains("[Alt+S]"));
    assert!(text_single.contains("[Alt+F]"));
    assert!(text_single.contains("[Tab]"));
    assert!(text_single.contains("[↑/↓]"));
    assert!(text_single.contains("[Ctrl+U]"));
    assert!(text_single.contains("[Esc]"));

    // 6. Full UI render on a 140x24 terminal verifies single-row footer allocation
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use sam_fuzzy::ui::render_ui;

    let backend_140 = TestBackend::new(140, 24);
    let mut terminal_140 = Terminal::new(backend_140).unwrap();
    terminal_140.draw(|f| render_ui(f, &app)).unwrap();
    let buffer_140 = terminal_140.backend().buffer();
    // The bottom-most row (line index 23) contains all shortcuts in a single line
    let bottom_row: String = (0..140)
        .map(|x| buffer_140.cell((x, 23)).unwrap().symbol().to_string())
        .collect();
    assert!(bottom_row.contains("[Enter]"));
    assert!(bottom_row.contains("[Alt+C]"));
    assert!(bottom_row.contains("[Alt+S]"));
    assert!(bottom_row.contains("[Tab]"));
    assert!(bottom_row.contains("[Esc]"));
}
