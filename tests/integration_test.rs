use sam_fuzzy::app::App;
use sam_fuzzy::fuzzy::SearchEngine;
use sam_fuzzy::models::load_dataset;
use std::path::Path;
use std::time::Instant;

#[test]
fn test_real_dataset_load_and_search() {
    let data_path = Path::new("data/sam_media.json");
    if !data_path.exists() {
        eprintln!("Warning: data/sam_media.json does not exist, skipping real dataset test.");
        return;
    }

    let start_load = Instant::now();
    let items = load_dataset(data_path).expect("Real dataset must load cleanly");
    let load_time = start_load.elapsed();
    println!("Loaded {} items in {:?}", items.len(), load_time);
    assert!(items.len() > 30000, "Dataset should have > 30k items");

    let engine = SearchEngine::new(items);
    assert!(engine.total_count() > 30000);

    // 1. Test search: "Kraven"
    let start_search = Instant::now();
    let results = engine.search("Kraven", "All", 20);
    let search_time = start_search.elapsed();
    println!(
        "Searched across {} items in {:?}",
        engine.total_count(),
        search_time
    );

    assert!(!results.is_empty());
    assert!(results[0].item.title.to_lowercase().contains("kraven"));
    // Under 80ms in debug mode across 104,650 items (sub-15ms in release)
    assert!(
        search_time.as_millis() < 80,
        "Search took too long: {:?}",
        search_time
    );

    // 2. Test search: "Spider-Man" in category "1080p"
    let spider_1080 = engine.search("Spider", "1080p", 10);
    assert!(!spider_1080.is_empty());
    for res in &spider_1080 {
        assert!(res.item.matches_category("1080p"));
    }

    // 3. Test App state machine with real engine
    let mut app = App::new(engine);
    app.query = "Witcher".to_string();
    app.perform_search();
    assert!(!app.results.is_empty());
    let top_match = app.selected_item().expect("Must have selected item");
    assert!(top_match.title.to_lowercase().contains("witcher"));

    // 4. Verify browser URL is a valid direct video file URL
    assert!(top_match.url.starts_with("http://172.16.50."));
    assert!(top_match.is_file, "All items in dataset must be direct video files");

    // 5. Test search: "squid game" on real dataset via App
    app.query = "squid game".to_string();
    app.perform_search();
    assert!(!app.results.is_empty(), "Must find Squid Game");
    let squid_top = app.selected_item().expect("Must have selected item");
    assert!(
        squid_top.title.to_lowercase().starts_with("squid game"),
        "Top match for 'squid game' must start with 'Squid Game', got: {}",
        squid_top.title
    );
    assert!(squid_top.url.contains("172.16.50.14"));
    assert!(squid_top.is_file, "Must be direct file");
}
