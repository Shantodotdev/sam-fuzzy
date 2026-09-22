use sam_fuzzy::actions::{ActionOutcome, DownloadEvent, parse_size_hint, start_download};
use sam_fuzzy::models::MediaItem;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
fn test_parse_size_hint_supports_index_units_and_unknown_values() {
    assert_eq!(parse_size_hint(Some("2.45 GB")), Some(2_630_667_469));
    assert_eq!(parse_size_hint(Some("815.7 MiB")), Some(855_323_443));
    assert_eq!(parse_size_hint(Some("512 B")), Some(512));
    assert_eq!(parse_size_hint(Some("unknown")), None);
    assert_eq!(parse_size_hint(None), None);
}

#[test]
fn test_native_parallel_downloader_reassembles_http_ranges() {
    let body: Vec<u8> = (0..(9 * 1024 * 1024 + 37))
        .map(|index| (index % 251) as u8)
        .collect();
    let expected_requests = 4; // One `bytes=0-0` probe plus three 4 MiB chunks.
    let (url, server) = spawn_range_server(Arc::new(body.clone()), expected_requests);
    let directory = std::env::temp_dir().join(format!(
        "sam-fuzzy-native-download-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let (updates, events) = channel();
    let cancellation = Arc::new(AtomicBool::new(false));
    let destination = start_download(
        1,
        url,
        "parallel.bin".to_string(),
        directory.clone(),
        updates,
        cancellation,
        Some(body.len() as u64),
    )
    .unwrap();

    let event = loop {
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            DownloadEvent::Completed { destination, .. } => break destination,
            DownloadEvent::Failed { error, .. } => panic!("parallel download failed: {error}"),
            DownloadEvent::Cancelled { .. } => panic!("parallel download was cancelled"),
            DownloadEvent::Progress { .. } => {}
        }
    };

    assert_eq!(event, destination);
    assert_eq!(std::fs::read(destination).unwrap(), body);
    server.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

fn spawn_range_server(
    body: Arc<Vec<u8>>,
    expected_requests: usize,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            respond_to_range_request(&mut stream, &body);
        }
    });
    (format!("http://{address}/file.bin"), server)
}

fn respond_to_range_request(stream: &mut TcpStream, body: &[u8]) {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "client closed request before headers completed");
        request.extend_from_slice(&buffer[..read]);
    }
    let request = String::from_utf8(request).unwrap();
    let range = request
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("range")
                .then(|| value.trim().strip_prefix("bytes="))?
        })
        .expect("range request expected");
    let (start, end) = range.split_once('-').unwrap();
    let start: usize = start.parse().unwrap();
    let end: usize = end.parse().unwrap();
    let data = &body[start..=end];
    write!(
        stream,
        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nConnection: close\r\n\r\n",
        data.len(),
        body.len()
    )
    .unwrap();
    stream.write_all(data).unwrap();
}

#[test]
fn test_url_target_selection() {
    let item = sample_item();
    assert_eq!(
        item.url,
        "http://172.16.50.7/DHAKA-FLIX-7/English%20Movies/Dune.mkv"
    );
    assert_eq!(
        item.folder_url,
        "http://172.16.50.7/DHAKA-FLIX-7/English%20Movies/"
    );
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
