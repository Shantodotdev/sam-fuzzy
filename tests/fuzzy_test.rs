use sam_fuzzy::fuzzy::SearchEngine;
use sam_fuzzy::models::MediaItem;
use std::time::Instant;

fn sample_items() -> Vec<MediaItem> {
    vec![
        MediaItem {
            id: 1,
            title: "The Dark Knight".to_string(),
            year: Some(2008),
            quality: "1080p BluRay".to_string(),
            category: "English Movies".to_string(),
            filename: "The.Dark.Knight.2008.1080p.mkv".to_string(),
            is_file: true,
            url: "http://172.16.50.7/dark_knight.mkv".to_string(),
            folder_url: "http://172.16.50.7/dark_knight/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/English Movies/dark_knight.mkv".to_string(),
        },
        MediaItem {
            id: 2,
            title: "The Dark Knight Rises".to_string(),
            year: Some(2012),
            quality: "1080p BluRay".to_string(),
            category: "English Movies".to_string(),
            filename: "The.Dark.Knight.Rises.2012.1080p.mkv".to_string(),
            is_file: true,
            url: "http://172.16.50.7/dark_knight_rises.mkv".to_string(),
            folder_url: "http://172.16.50.7/dark_knight_rises/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/English Movies/dark_knight_rises.mkv".to_string(),
        },
        MediaItem {
            id: 3,
            title: "Spider-Man Into the Spider-Verse".to_string(),
            year: Some(2018),
            quality: "1080p [3D]".to_string(),
            category: "3D Movies".to_string(),
            filename: "Spider-Man Into the Spider-Verse 2018 3D.mkv".to_string(),
            is_file: true,
            url: "http://172.16.50.7/spiderman_verse.mkv".to_string(),
            folder_url: "http://172.16.50.7/spiderman_verse/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/3D Movies/spiderman_verse.mkv".to_string(),
        },
        MediaItem {
            id: 4,
            title: "Feluda 50".to_string(),
            year: Some(2017),
            quality: "720p".to_string(),
            category: "Kolkata Bangla Movies".to_string(),
            filename: "Feluda 50.mkv".to_string(),
            is_file: true,
            url: "http://172.16.50.7/feluda.mkv".to_string(),
            folder_url: "http://172.16.50.7/feluda/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/Kolkata Bangla Movies/feluda.mkv".to_string(),
        },
    ]
}

#[test]
fn test_search_empty_query_returns_all() {
    let items = sample_items();
    let engine = SearchEngine::new(items);
    let results = engine.search("", "All", 10);
    assert_eq!(results.len(), 4);
}

#[test]
fn test_search_fuzzy_matching_and_ranking() {
    let items = sample_items();
    let engine = SearchEngine::new(items);
    let results = engine.search("dark knight", "All", 10);

    assert!(results.len() >= 2);
    // Both Dark Knight movies should match and appear first
    assert!(results[0].item.title.contains("Dark Knight"));
    assert!(results[1].item.title.contains("Dark Knight"));
}

#[test]
fn test_search_with_category_filter() {
    let items = sample_items();
    let engine = SearchEngine::new(items);

    let results = engine.search("spider", "3D", 10);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].item.title, "Spider-Man Into the Spider-Verse");

    let no_results = engine.search("spider", "Bangla", 10);
    assert_eq!(no_results.len(), 0);
}

#[test]
fn test_search_performance_sub_millisecond() {
    // Generate 1000 items to test sub-millisecond fuzzy query speed
    let mut items = Vec::with_capacity(1000);
    for i in 0..1000 {
        items.push(MediaItem {
            id: i,
            title: format!("Movie Title Sequence {}", i),
            year: Some(2000 + (i % 26) as u16),
            quality: "1080p BluRay".to_string(),
            category: "English Movies".to_string(),
            filename: format!("Movie.Title.{}.mkv", i),
            is_file: true,
            url: format!("http://172.16.50.7/movie_{}.mkv", i),
            folder_url: "http://172.16.50.7/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: format!("DHAKA-FLIX-7/English Movies/movie_{}.mkv", i),
        });
    }

    let engine = SearchEngine::new(items);

    let start = Instant::now();
    let results = engine.search("sequence 50", "All", 20);
    let elapsed = start.elapsed();

    assert!(!results.is_empty());
    // Fuzzy matching over 1000 items must be well under 10ms (typically under 1ms)
    assert!(
        elapsed.as_millis() < 10,
        "Fuzzy search took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_exact_lowercase_keyword_priority() {
    let items = vec![
        MediaItem {
            id: 1,
            title: "The Paradise of Thorns".to_string(),
            year: Some(2024),
            quality: "1080p".to_string(),
            category: "Foreign Language Movies".to_string(),
            filename: "The.Paradise.of.Thorns.2024.1080p.mkv".to_string(),
            is_file: false,
            url: "http://172.16.50.7/thorns/".to_string(),
            folder_url: "http://172.16.50.7/thorns/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/Foreign/Thorns".to_string(),
        },
        MediaItem {
            id: 2,
            title: "Thor".to_string(),
            year: Some(2011),
            quality: "1080p".to_string(),
            category: "English Movies".to_string(),
            filename: "Thor.2011.1080p.mkv".to_string(),
            is_file: true,
            url: "http://172.16.50.7/thor.mkv".to_string(),
            folder_url: "http://172.16.50.7/thor/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/English/Thor".to_string(),
        },
        MediaItem {
            id: 3,
            title: "Thor-Love and Thunder".to_string(),
            year: Some(2022),
            quality: "720p".to_string(),
            category: "English Movies".to_string(),
            filename: "Thor.Love.and.Thunder.2022.mkv".to_string(),
            is_file: true,
            url: "http://172.16.50.7/thor_love.mkv".to_string(),
            folder_url: "http://172.16.50.7/thor_love/".to_string(),
            server: "DHAKA-FLIX-7".to_string(),
            path: "DHAKA-FLIX-7/English/Thor-Love and Thunder".to_string(),
        },
    ];

    let engine = SearchEngine::new(items);
    let results = engine.search("thor", "All", 10);

    assert_eq!(
        results[0].item.title, "Thor",
        "Exact match 'Thor' must be #1"
    );
    assert_eq!(
        results[1].item.title, "Thor-Love and Thunder",
        "'Thor-Love and Thunder' must be #2"
    );
    assert_eq!(
        results[2].item.title, "The Paradise of Thorns",
        "'The Paradise of Thorns' should be lower priority"
    );
}
