use sam_fuzzy::actions::ActionOutcome;
use sam_fuzzy::models::MediaItem;

fn sample_item() -> MediaItem {
    MediaItem {
        id: 1,
        title: "Dune Part Two".to_string(),
        year: Some(2024),
        quality: "1080p BluRay".to_string(),
        category: "English Movies".to_string(),
        filename: "Dune.Part.Two.2024.1080p.mkv".to_string(),
        is_file: true,
        url: "http://172.16.50.7/DHAKA-FLIX-7/English%20Movies/Dune.mkv".to_string(),
        folder_url: "http://172.16.50.7/DHAKA-FLIX-7/English%20Movies/".to_string(),
        server: "DHAKA-FLIX-7".to_string(),
        path: "DHAKA-FLIX-7/English Movies/Dune.mkv".to_string(),
        size: Some("2.45 GB".to_string()),
    }
}

#[test]
fn test_action_outcome_format() {
    let outcome = ActionOutcome::BrowserOpened("http://172.16.50.7/test.mkv".to_string());
    assert!(outcome.message().contains("Opened in browser"));

    let outcome_clip = ActionOutcome::CopiedToClipboard("http://172.16.50.7/test.mkv".to_string());
    assert!(outcome_clip.message().contains("Copied link"));

    let outcome_player = ActionOutcome::PlayerLaunched("mpv".to_string());
    assert!(outcome_player.message().contains("mpv"));
}

#[test]
fn test_url_target_selection() {
    let item = sample_item();
    assert_eq!(item.url, "http://172.16.50.7/DHAKA-FLIX-7/English%20Movies/Dune.mkv");
    assert_eq!(item.folder_url, "http://172.16.50.7/DHAKA-FLIX-7/English%20Movies/");
}

#[test]
fn test_app_action_dispatch_on_selection() {
    use sam_fuzzy::app::App;
    use sam_fuzzy::fuzzy::SearchEngine;

    let engine = SearchEngine::new(vec![sample_item()]);
    let mut app = App::new(engine);

    assert_eq!(app.selected_item().unwrap().title, "Dune Part Two");

    // Calling copy_selected_link sets status
    let outcome = app.copy_selected_link();
    assert!(outcome.is_some());
    assert!(app.status.is_some());
}

