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
    assert!(row_strings.iter().any(|r| r.contains("1 ") && !r.contains("1. ")));
    assert!(row_strings.iter().any(|r| r.contains("2 ") && !r.contains("2. ")));
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
    let row_single = row_strings.iter().find(|r| r.contains("Movie Episode 02")).unwrap();
    let row_double = row_strings.iter().find(|r| r.contains("Movie Episode 10")).unwrap();

    let col_single = row_single.find("Movie Episode 02").unwrap();
    let col_double = row_double.find("Movie Episode 10").unwrap();

    assert_eq!(
        col_single, col_double,
        "Single-digit and double-digit items must start at the exact same column ({col_single} vs {col_double})"
    );
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



