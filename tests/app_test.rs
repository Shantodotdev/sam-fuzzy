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
