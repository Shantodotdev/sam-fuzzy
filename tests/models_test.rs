use sam_fuzzy::models::{load_dataset_from_str, MediaItem};

#[test]
fn test_media_item_display_title() {
    let item = MediaItem {
        id: 1,
        title: "Kraven the Hunter".to_string(),
        year: Some(2024),
        quality: "720p / AMZN".to_string(),
        category: "English Movies".to_string(),
        filename: "Kraven.mkv".to_string(),
        is_file: true,
        url: "http://172.16.50.7/test.mkv".to_string(),
        folder_url: "http://172.16.50.7/".to_string(),
        server: "DHAKA-FLIX-7".to_string(),
        path: "DHAKA-FLIX-7/English Movies/test.mkv".to_string(),
    };

    assert_eq!(item.display_title(), "Kraven the Hunter (2024)");
}

#[test]
fn test_media_item_display_title_without_year() {
    let item = MediaItem {
        id: 2,
        title: "Mystery Film".to_string(),
        year: None,
        quality: "1080p".to_string(),
        category: "English Movies".to_string(),
        filename: "Mystery.mkv".to_string(),
        is_file: true,
        url: "http://172.16.50.7/test2.mkv".to_string(),
        folder_url: "http://172.16.50.7/".to_string(),
        server: "DHAKA-FLIX-7".to_string(),
        path: "DHAKA-FLIX-7/English Movies/test2.mkv".to_string(),
    };

    assert_eq!(item.display_title(), "Mystery Film");
}

#[test]
fn test_search_haystack_contains_metadata() {
    let item = MediaItem {
        id: 3,
        title: "Doctor Strange".to_string(),
        year: Some(2016),
        quality: "1080p [3D]".to_string(),
        category: "3D Movies".to_string(),
        filename: "Doctor Strange 2016 3D.mkv".to_string(),
        is_file: true,
        url: "http://172.16.50.7/doc.mkv".to_string(),
        folder_url: "http://172.16.50.7/".to_string(),
        server: "DHAKA-FLIX-7".to_string(),
        path: "DHAKA-FLIX-7/3D Movies/doc.mkv".to_string(),
    };

    let haystack = item.search_haystack().to_lowercase();
    assert!(haystack.contains("doctor strange"));
    assert!(haystack.contains("2016"));
    assert!(haystack.contains("3d movies"));
    assert!(haystack.contains("1080p"));
}

#[test]
fn test_category_matching() {
    let english_item = MediaItem {
        id: 1,
        title: "Inception".to_string(),
        year: Some(2010),
        quality: "1080p".to_string(),
        category: "English Movies".to_string(),
        filename: "Inception.mkv".to_string(),
        is_file: true,
        url: "http://172.16.50.7/inc.mkv".to_string(),
        folder_url: "http://172.16.50.7/".to_string(),
        server: "DHAKA-FLIX-7".to_string(),
        path: "DHAKA-FLIX-7/English Movies/inc.mkv".to_string(),
    };

    let bangla_item = MediaItem {
        id: 2,
        title: "Chander Pahar".to_string(),
        year: Some(2013),
        quality: "720p".to_string(),
        category: "Kolkata Bangla Movies".to_string(),
        filename: "Chander Pahar.mkv".to_string(),
        is_file: true,
        url: "http://172.16.50.7/cp.mkv".to_string(),
        folder_url: "http://172.16.50.7/".to_string(),
        server: "DHAKA-FLIX-7".to_string(),
        path: "DHAKA-FLIX-7/Kolkata Bangla Movies/cp.mkv".to_string(),
    };

    let korean_item = MediaItem {
        id: 3,
        title: "Parasite".to_string(),
        year: Some(2019),
        quality: "1080p".to_string(),
        category: "Foreign Language Movies / Korean Language".to_string(),
        filename: "Parasite.mkv".to_string(),
        is_file: false,
        url: "http://172.16.50.7/parasite/".to_string(),
        folder_url: "http://172.16.50.7/parasite/".to_string(),
        server: "DHAKA-FLIX-7".to_string(),
        path: "DHAKA-FLIX-7/Foreign Language Movies/Korean Language/Parasite".to_string(),
    };

    assert!(english_item.matches_category("All"));
    assert!(english_item.matches_category("English"));
    assert!(!english_item.matches_category("Bangla"));

    assert!(bangla_item.matches_category("All"));
    assert!(bangla_item.matches_category("Bangla"));
    assert!(!bangla_item.matches_category("English"));

    assert!(korean_item.matches_category("All"));
    assert!(korean_item.matches_category("Korean"));
    assert!(korean_item.matches_category("Foreign"));
    assert!(!korean_item.matches_category("Files Only"));
    assert!(korean_item.matches_category("Folders Only"));
}

#[test]
fn test_load_dataset_from_str() {
    let json_data = r#"[
        {
            "id": 1,
            "title": "Gladiator",
            "year": 2000,
            "quality": "1080p",
            "category": "English Movies",
            "filename": "Gladiator.mkv",
            "is_file": true,
            "url": "http://172.16.50.7/gladiator.mkv",
            "folder_url": "http://172.16.50.7/",
            "server": "DHAKA-FLIX-7",
            "path": "DHAKA-FLIX-7/English Movies/gladiator.mkv"
        }
    ]"#;

    let items = load_dataset_from_str(json_data).expect("Should parse dataset successfully");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Gladiator");
    assert_eq!(items[0].year, Some(2000));
}

#[test]
fn test_natural_cmp_series_ordering() {
    use sam_fuzzy::models::natural_cmp;

    assert_eq!(natural_cmp("Squid Game S01E01", "Squid Game S01E02"), std::cmp::Ordering::Less);
    assert_eq!(natural_cmp("Squid Game S01E02", "Squid Game S01E10"), std::cmp::Ordering::Less);
    assert_eq!(natural_cmp("Squid Game S01E09", "Squid Game S01E10"), std::cmp::Ordering::Less);
    assert_eq!(natural_cmp("Squid Game S01E10", "Squid Game S02E01"), std::cmp::Ordering::Less);
    assert_eq!(natural_cmp("Squid Game S02E06", "Squid Game S03E01"), std::cmp::Ordering::Less);
}

#[test]
fn test_display_title_strips_mkv_and_extensions() {
    let item = MediaItem {
        id: 10,
        title: "Squid Game S01E01.mkv".to_string(),
        year: None,
        quality: "1080p".to_string(),
        category: "TV Series".to_string(),
        filename: "Squid Game S01E01.mkv".to_string(),
        is_file: true,
        url: "http://172.16.50.14/squid.mkv".to_string(),
        folder_url: "http://172.16.50.14/".to_string(),
        server: "DHAKA-FLIX-14".to_string(),
        path: "DHAKA-FLIX-14/squid.mkv".to_string(),
    };

    assert_eq!(item.display_title(), "Squid Game S01E01");
}
