use clap::Parser;
use sam_fuzzy::cli::{Filters, SearchArgs, parse_size_arg, parse_year_range};
use sam_fuzzy::fuzzy::SearchEngine;
use sam_fuzzy::models::MediaItem;

#[derive(Parser)]
struct Wrapper {
    #[command(flatten)]
    args: SearchArgs,
}

fn item(id: usize, title: &str, year: u16, quality: &str, file: &str, size: &str) -> MediaItem {
    MediaItem {
        id,
        title: title.into(),
        year: Some(year),
        quality: quality.into(),
        category: "English Movies".into(),
        filename: file.into(),
        is_file: true,
        url: format!("http://localhost/{file}"),
        folder_url: "http://localhost/".into(),
        server: "DHAKA-FLIX-7".into(),
        path: file.into(),
        size: Some(size.into()),
    }
}

fn filters(args: &[&str]) -> Filters {
    let mut argv = vec!["x"];
    argv.extend_from_slice(args);
    Filters::from_args(&Wrapper::parse_from(argv).args).unwrap()
}

#[test]
fn year_and_size_parsing() {
    assert_eq!(parse_year_range("2019").unwrap(), (Some(2019), Some(2019)));
    assert_eq!(
        parse_year_range("2015-2020").unwrap(),
        (Some(2015), Some(2020))
    );
    assert_eq!(parse_year_range("2015-").unwrap(), (Some(2015), None));
    assert_eq!(parse_year_range("-2020").unwrap(), (None, Some(2020)));
    assert!(parse_year_range("2020-2010").is_err());
    assert_eq!(parse_size_arg("1.5GB").unwrap(), 1_610_612_736);
    assert_eq!(parse_size_arg("500 MB").unwrap(), 524_288_000);
    assert!(parse_size_arg("lots").is_err());
}

#[test]
fn filters_narrow_results() {
    let items = vec![
        item(
            1,
            "Heat",
            1995,
            "1080p / BluRay / HEVC",
            "Heat.1995.1080p.BluRay.HEVC.mkv",
            "3.0 GB",
        ),
        item(
            2,
            "Heat",
            1995,
            "720p / WEBRip / x264",
            "Heat.1995.720p.WEBRip.x264.mp4",
            "900 MB",
        ),
        item(
            3,
            "Heat",
            2013,
            "720p / BluRay / Dual Audio / x264",
            "Heat.2013.720p.mkv",
            "1.2 GB",
        ),
    ];
    let engine = SearchEngine::new(items);
    let ids = |args: &[&str]| -> Vec<usize> {
        let f = filters(args);
        let mut ids: Vec<usize> = engine
            .search_with("heat", "All", &|i| f.matches(i))
            .iter()
            .map(|(i, _)| i.id)
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&[]), vec![1, 2, 3]);
    assert_eq!(ids(&["--year", "1990-2000"]), vec![1, 2]);
    assert_eq!(ids(&["-r", "720p", "--type", "mp4"]), vec![2]);
    assert_eq!(ids(&["--codec", "hevc"]), vec![1]);
    assert_eq!(ids(&["--source", "bluray", "--max-size", "2GB"]), vec![3]);
    assert_eq!(ids(&["--dual-audio"]), vec![3]);
}

#[test]
fn invalid_filter_values_are_rejected() {
    let args = Wrapper::parse_from(["x", "-r", "8k"]).args;
    assert!(Filters::from_args(&args).is_err());
    let args = Wrapper::parse_from(["x", "--codec", "divx"]).args;
    assert!(Filters::from_args(&args).is_err());
}
