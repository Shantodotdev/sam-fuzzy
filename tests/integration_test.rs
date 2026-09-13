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
    // Sub-millisecond or low single digit ms
    assert!(
        search_time.as_millis() < 25,
        "Search took too long: {:?}",
        search_time
    );

    // 2. Test search: "Spider-Man" in category "3D"
    let spider_3d = engine.search("Spider", "3D", 10);
    assert!(!spider_3d.is_empty());
    for res in &spider_3d {
        assert!(res.item.matches_category("3D"));
    }

    // 3. Test App state machine with real engine
    let mut app = App::new(engine);
    app.query = "Witcher".to_string();
    app.perform_search();
    assert!(!app.results.is_empty());
    let top_match = app.selected_item().expect("Must have selected item");
    assert!(top_match.title.to_lowercase().contains("witcher"));

    // 4. Verify browser URL is a valid http:// URL
    assert!(top_match.url.starts_with("http://172.16.50."));

    // 5. Test search: "squid game" on real dataset via App
    app.query = "squid game".to_string();
    app.perform_search();
    assert!(!app.results.is_empty(), "Must find Squid Game");
    let squid_top = app.selected_item().expect("Must have selected item");
    assert_eq!(
        squid_top.title, "Squid Game",
        "Top match for 'squid game' must be 'Squid Game'"
    );
    assert!(squid_top.url.contains("172.16.50.14"));
}
