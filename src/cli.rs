//! Non-interactive, agent-friendly command line interface.
//!
//! Every command here is headless: it never touches the terminal UI, writes machine-readable
//! JSON to stdout when stdout is not a terminal, and reports problems through the exit code and a
//! JSON `{"error": ...}` object on stderr. Workflow for an agent:
//!
//! 1. `sam-fuzzy categories` – discover category filters.
//! 2. `sam-fuzzy search "inception" --resolution 1080p --group` – find what is available.
//! 3. `sam-fuzzy download <id>...` – fetch the chosen file ids.

use crate::actions::{DownloadEvent, default_download_dir, start_download};
use crate::fuzzy::SearchEngine;
use crate::models::MediaItem;
use anyhow::{Context, bail};
use clap::{Args, ValueEnum};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// Hard ceiling for `--limit`.
pub const MAX_LIMIT: usize = 1000;

/// How command results are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Table when stdout is a terminal, JSON otherwise.
    Auto,
    /// One JSON document (the default for pipes and agents).
    Json,
    /// One JSON object per result line.
    Jsonl,
    /// Aligned human-readable table.
    Table,
    /// Only item ids, one per line (pipe into `download`).
    Ids,
}

impl OutputFormat {
    fn resolve(self) -> OutputFormat {
        match self {
            OutputFormat::Auto if std::io::stdout().is_terminal() => OutputFormat::Table,
            OutputFormat::Auto => OutputFormat::Json,
            other => other,
        }
    }
}

/// Result ordering for `search`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SortKey {
    /// Fuzzy relevance (alphabetical when the query is empty).
    Relevance,
    /// Title A-Z.
    Title,
    /// Newest release year first.
    Year,
    /// Largest file first.
    Size,
}

/// Arguments for `sam-fuzzy search`.
#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Fuzzy query, e.g. "dark knight". Empty lists everything matching the filters.
    pub query: Option<String>,

    /// Category filter (see `sam-fuzzy categories`): English, "TV Series", Korean, Hindi,
    /// Animation, Bangla, Foreign, "Chinese/Japanese", 3D, or any category substring.
    #[arg(short, long, default_value = "All")]
    pub category: String,

    /// Release year: `2019`, a range `2015-2020`, or open-ended `2015-` / `-2020`.
    #[arg(short, long)]
    pub year: Option<String>,

    /// Resolution(s): 4k, 1080p, 720p, 576p, 480p, 360p. Repeat or comma-separate.
    #[arg(short, long, value_delimiter = ',')]
    pub resolution: Vec<String>,

    /// File type(s) by extension: mkv, mp4, avi... Repeat or comma-separate.
    #[arg(short = 't', long = "type", value_delimiter = ',')]
    pub file_type: Vec<String>,

    /// Codec(s): hevc (x265), x264, av1.
    #[arg(long, value_delimiter = ',')]
    pub codec: Vec<String>,

    /// Source(s): bluray, web, hdtv, dvd, hdrip, cam.
    #[arg(long, value_delimiter = ',')]
    pub source: Vec<String>,

    /// Only releases that advertise dual audio.
    #[arg(long)]
    pub dual_audio: bool,

    /// Minimum file size, e.g. 500MB or 1.5GB.
    #[arg(long)]
    pub min_size: Option<String>,

    /// Maximum file size, e.g. 2GB.
    #[arg(long)]
    pub max_size: Option<String>,

    /// Only files on this server (substring), e.g. DHAKA-FLIX-14.
    #[arg(long)]
    pub server: Option<String>,

    /// Collapse files of the same title and year into one entry with a `variants` list.
    #[arg(short, long)]
    pub group: bool,

    /// Order of results.
    #[arg(short, long, value_enum, default_value = "relevance")]
    pub sort: SortKey,

    /// Reverse the sort order.
    #[arg(long)]
    pub reverse: bool,

    /// Maximum results returned (1-1000).
    #[arg(short, long, default_value_t = 20, value_parser = clap::value_parser!(u16).range(1..=1000))]
    pub limit: u16,

    /// Skip this many results first (pagination).
    #[arg(long, default_value_t = 0)]
    pub offset: usize,

    /// Maximum variants listed per group when using --group.
    #[arg(long, default_value_t = 25)]
    pub max_variants: usize,

    /// Output format.
    #[arg(short, long, value_enum, default_value = "auto")]
    pub format: OutputFormat,
}

/// Arguments for `sam-fuzzy show`.
#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Item id as returned by `search`.
    pub id: usize,

    /// Output format.
    #[arg(short, long, value_enum, default_value = "auto")]
    pub format: OutputFormat,
}

/// Arguments for `sam-fuzzy categories`.
#[derive(Args, Debug)]
pub struct CategoriesArgs {
    /// Output format.
    #[arg(short, long, value_enum, default_value = "auto")]
    pub format: OutputFormat,
}

/// Arguments for `sam-fuzzy download`.
#[derive(Args, Debug)]
pub struct DownloadArgs {
    /// One or more item ids from `search`.
    #[arg(required = true, num_args = 1..)]
    pub ids: Vec<usize>,

    /// Directory to save into (defaults to your Downloads folder).
    #[arg(short, long = "output")]
    pub output: Option<PathBuf>,

    /// Files downloaded at the same time (each already uses parallel range workers).
    #[arg(short, long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=4))]
    pub jobs: u8,

    /// Show what would be downloaded without fetching anything.
    #[arg(long)]
    pub dry_run: bool,

    /// Do not download a file whose name already exists in the output directory.
    #[arg(long)]
    pub skip_existing: bool,

    /// Suppress progress output on stderr.
    #[arg(short, long)]
    pub quiet: bool,

    /// Output format for the final report.
    #[arg(short, long, value_enum, default_value = "auto")]
    pub format: OutputFormat,
}

/// Parsed, validated search filters.
#[derive(Debug, Default)]
pub struct Filters {
    year_from: Option<u16>,
    year_to: Option<u16>,
    resolutions: Vec<String>,
    types: Vec<String>,
    codecs: Vec<&'static str>,
    sources: Vec<&'static str>,
    dual_audio: bool,
    min_size: Option<u64>,
    max_size: Option<u64>,
    server: Option<String>,
}

impl Filters {
    /// Validates the raw command-line values.
    pub fn from_args(args: &SearchArgs) -> anyhow::Result<Self> {
        let (year_from, year_to) = match args.year.as_deref() {
            Some(value) => parse_year_range(value)?,
            None => (None, None),
        };
        let resolutions = args
            .resolution
            .iter()
            .map(|r| normalize_resolution(r))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let codecs = args
            .codec
            .iter()
            .map(|c| {
                normalize_choice(
                    c,
                    &[
                        ("HEVC", &["hevc", "x265", "h265", "h.265"]),
                        ("x264", &["x264", "h264", "h.264", "avc"]),
                        ("AV1", &["av1"]),
                    ],
                    "codec",
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let sources = args
            .source
            .iter()
            .map(|s| {
                normalize_choice(
                    s,
                    &[
                        ("BluRay", &["bluray", "blu-ray", "brrip", "bdrip"]),
                        ("WEB", &["web", "webrip", "web-dl", "webdl"]),
                        ("HDTV", &["hdtv"]),
                        ("DVD", &["dvd", "dvdrip"]),
                        ("HDRip", &["hdrip"]),
                        ("CAM", &["cam", "hdcam", "ts"]),
                    ],
                    "source",
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let types = args
            .file_type
            .iter()
            .map(|t| t.trim().trim_start_matches('.').to_ascii_lowercase())
            .collect();
        let min_size = args.min_size.as_deref().map(parse_size_arg).transpose()?;
        let max_size = args.max_size.as_deref().map(parse_size_arg).transpose()?;
        if let (Some(min), Some(max)) = (min_size, max_size)
            && min > max
        {
            bail!("--min-size is larger than --max-size");
        }
        Ok(Self {
            year_from,
            year_to,
            resolutions,
            types,
            codecs,
            sources,
            dual_audio: args.dual_audio,
            min_size,
            max_size,
            server: args.server.as_ref().map(|s| s.to_ascii_lowercase()),
        })
    }

    /// True when `item` satisfies every active filter.
    pub fn matches(&self, item: &MediaItem) -> bool {
        if self.year_from.is_some() || self.year_to.is_some() {
            let Some(year) = item.year else {
                return false;
            };
            if self.year_from.is_some_and(|from| year < from)
                || self.year_to.is_some_and(|to| year > to)
            {
                return false;
            }
        }
        if !self.resolutions.is_empty()
            && !item
                .clean_resolution()
                .is_some_and(|r| self.resolutions.iter().any(|want| want == r))
        {
            return false;
        }
        if !self.types.is_empty()
            && !item
                .extension()
                .is_some_and(|ext| self.types.contains(&ext))
        {
            return false;
        }
        if !self.codecs.is_empty() && !item.codec().is_some_and(|c| self.codecs.contains(&c)) {
            return false;
        }
        if !self.sources.is_empty() && !item.source().is_some_and(|s| self.sources.contains(&s)) {
            return false;
        }
        if self.dual_audio && !item.is_dual_audio() {
            return false;
        }
        if self.min_size.is_some() || self.max_size.is_some() {
            let Some(bytes) = item.size_bytes() else {
                return false;
            };
            if self.min_size.is_some_and(|min| bytes < min)
                || self.max_size.is_some_and(|max| bytes > max)
            {
                return false;
            }
        }
        if let Some(server) = &self.server
            && !item.server.to_ascii_lowercase().contains(server)
        {
            return false;
        }
        true
    }
}

/// Parses `2019`, `2015-2020`, `2015-` or `-2020`.
pub fn parse_year_range(value: &str) -> anyhow::Result<(Option<u16>, Option<u16>)> {
    let parse = |part: &str| -> anyhow::Result<Option<u16>> {
        let part = part.trim();
        if part.is_empty() {
            return Ok(None);
        }
        part.parse::<u16>()
            .map(Some)
            .with_context(|| format!("invalid year '{part}' (expected e.g. 2019, 2015-2020)"))
    };
    let (from, to) = match value.split_once('-') {
        Some((from, to)) => (parse(from)?, parse(to)?),
        None => {
            let year = parse(value)?;
            (year, year)
        }
    };
    if from.is_none() && to.is_none() {
        bail!("invalid year range '{value}'");
    }
    if let (Some(from), Some(to)) = (from, to)
        && from > to
    {
        bail!("year range '{value}' starts after it ends");
    }
    Ok((from, to))
}

/// Parses sizes such as `500MB`, `1.5 GB` or `700000` (bytes) into bytes.
pub fn parse_size_arg(value: &str) -> anyhow::Result<u64> {
    let value = value.trim();
    let split = value
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(value.len());
    let (amount, unit) = value.split_at(split);
    let unit = if unit.trim().is_empty() {
        "b"
    } else {
        unit.trim()
    };
    crate::actions::parse_size_hint(Some(&format!("{amount} {unit}")))
        .with_context(|| format!("invalid size '{value}' (expected e.g. 500MB, 1.5GB)"))
}

fn normalize_resolution(value: &str) -> anyhow::Result<String> {
    let lower = value.trim().to_ascii_lowercase();
    let digits = lower.trim_end_matches('p');
    match digits {
        "4k" | "uhd" | "2160" => Ok("4K".to_string()),
        "1080" | "720" | "576" | "480" | "360" => Ok(format!("{digits}p")),
        _ => bail!("unknown resolution '{value}' (use 4k, 1080p, 720p, 576p, 480p, 360p)"),
    }
}

fn normalize_choice(
    value: &str,
    table: &[(&'static str, &[&str])],
    what: &str,
) -> anyhow::Result<&'static str> {
    let lower = value.trim().to_ascii_lowercase();
    for (label, aliases) in table {
        if aliases.contains(&lower.as_str()) {
            return Ok(label);
        }
    }
    let valid: Vec<&str> = table.iter().map(|(label, _)| *label).collect();
    bail!("unknown {what} '{value}' (valid: {})", valid.join(", "))
}

/// Serializable view of one media file.
#[derive(Debug, Serialize)]
pub struct ItemView {
    pub id: usize,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'static str>,
    pub dual_audio: bool,
    pub quality: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub category: String,
    pub filename: String,
    pub server: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<u64>,
}

impl ItemView {
    fn new(item: &MediaItem, score: Option<u64>) -> Self {
        Self {
            id: item.id,
            title: item.title.clone(),
            year: item.year,
            resolution: item.clean_resolution(),
            codec: item.codec(),
            source: item.source(),
            dual_audio: item.is_dual_audio(),
            quality: item.quality.clone(),
            file_type: item.extension(),
            size: item.size.clone(),
            size_bytes: item.size_bytes(),
            category: item.category.clone(),
            filename: item.filename.clone(),
            server: item.server.clone(),
            url: item.url.clone(),
            score,
        }
    }
}

/// One title (movie or show) with all its available files.
#[derive(Debug, Serialize)]
pub struct GroupView {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
    pub category: String,
    pub variant_count: usize,
    pub variants: Vec<ItemView>,
}

/// `search` response envelope.
#[derive(Debug, Serialize)]
pub struct SearchResponse<T: Serialize> {
    pub query: String,
    pub total_matches: usize,
    pub returned: usize,
    pub offset: usize,
    pub has_more: bool,
    pub grouped: bool,
    pub results: Vec<T>,
}

/// Grouping key: lowercase title plus release year.
type GroupKey = (String, Option<u16>);

/// Runs `search` and prints the result.
pub fn run_search(engine: &SearchEngine, args: &SearchArgs) -> anyhow::Result<()> {
    let filters = Filters::from_args(args)?;
    let query = args.query.clone().unwrap_or_default();
    let mut ranked = engine.search_with(&query, &args.category, &|item| {
        item.is_file && filters.matches(item)
    });

    sort_ranked(&mut ranked, args.sort, args.reverse);

    let limit = args.limit as usize;
    let format = args.format.resolve();
    let out = std::io::stdout();
    let mut out = out.lock();

    if args.group {
        let mut order: Vec<GroupKey> = Vec::new();
        let mut groups: HashMap<GroupKey, Vec<(&MediaItem, u64)>> = HashMap::new();
        for entry in &ranked {
            let key = (entry.0.title.to_lowercase(), entry.0.year);
            groups
                .entry(key.clone())
                .or_insert_with(|| {
                    order.push(key);
                    Vec::new()
                })
                .push(*entry);
        }
        let total = order.len();
        let page: Vec<GroupView> = order
            .into_iter()
            .skip(args.offset)
            .take(limit)
            .map(|key| {
                let files = &groups[&key];
                let first = files[0].0;
                GroupView {
                    title: first.title.clone(),
                    year: first.year,
                    category: first.category.clone(),
                    variant_count: files.len(),
                    variants: files
                        .iter()
                        .take(args.max_variants)
                        .map(|(item, score)| ItemView::new(item, scored(&query, *score)))
                        .collect(),
                }
            })
            .collect();
        return print_groups(&mut out, format, &query, total, args.offset, page);
    }

    let total = ranked.len();
    let page: Vec<ItemView> = ranked
        .iter()
        .skip(args.offset)
        .take(limit)
        .map(|(item, score)| ItemView::new(item, scored(&query, *score)))
        .collect();
    print_items(&mut out, format, &query, total, args.offset, page)
}

fn scored(query: &str, score: u64) -> Option<u64> {
    (!query.trim().is_empty()).then_some(score)
}

fn sort_ranked(ranked: &mut [(&MediaItem, u64)], sort: SortKey, reverse: bool) {
    use std::cmp::Ordering;
    // Option comparison that always keeps missing values last, regardless of direction.
    fn none_last<T: Ord>(a: Option<T>, b: Option<T>, descending: bool) -> Ordering {
        match (a, b) {
            (Some(a), Some(b)) if descending => b.cmp(&a),
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
    }
    match sort {
        SortKey::Relevance => {
            if reverse {
                ranked.reverse();
            }
        }
        SortKey::Title => ranked.sort_by(|a, b| {
            let ord = crate::models::natural_cmp(&a.0.title, &b.0.title);
            if reverse { ord.reverse() } else { ord }
        }),
        SortKey::Year => ranked.sort_by(|a, b| none_last(a.0.year, b.0.year, !reverse)),
        SortKey::Size => {
            ranked.sort_by(|a, b| none_last(a.0.size_bytes(), b.0.size_bytes(), !reverse))
        }
    }
}

fn print_items(
    out: &mut impl Write,
    format: OutputFormat,
    query: &str,
    total: usize,
    offset: usize,
    page: Vec<ItemView>,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Ids => {
            for item in &page {
                writeln!(out, "{}", item.id)?;
            }
        }
        OutputFormat::Jsonl => {
            for item in &page {
                writeln!(out, "{}", serde_json::to_string(item)?)?;
            }
        }
        OutputFormat::Table => {
            writeln!(
                out,
                "{:>7}  {:<44} {:>5}  {:<6} {:<6} {:<7} {:>9}  {:<4} CATEGORY",
                "ID", "TITLE", "YEAR", "RES", "CODEC", "SOURCE", "SIZE", "TYPE"
            )?;
            for item in &page {
                writeln!(out, "{}", item_row(item))?;
            }
            writeln!(
                out,
                "\n{} of {} matches (offset {offset})",
                page.len(),
                total
            )?;
        }
        _ => {
            let response = SearchResponse {
                query: query.to_string(),
                total_matches: total,
                returned: page.len(),
                offset,
                has_more: offset + page.len() < total,
                grouped: false,
                results: page,
            };
            writeln!(out, "{}", serde_json::to_string_pretty(&response)?)?;
        }
    }
    Ok(())
}

fn print_groups(
    out: &mut impl Write,
    format: OutputFormat,
    query: &str,
    total: usize,
    offset: usize,
    page: Vec<GroupView>,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Ids => {
            for group in &page {
                for item in &group.variants {
                    writeln!(out, "{}", item.id)?;
                }
            }
        }
        OutputFormat::Jsonl => {
            for group in &page {
                writeln!(out, "{}", serde_json::to_string(group)?)?;
            }
        }
        OutputFormat::Table => {
            for group in &page {
                let year = group.year.map(|y| format!(" ({y})")).unwrap_or_default();
                writeln!(
                    out,
                    "{}{}  [{}]  {} file(s)",
                    group.title, year, group.category, group.variant_count
                )?;
                for item in &group.variants {
                    writeln!(out, "  {}", item_row(item))?;
                }
            }
            writeln!(
                out,
                "\n{} of {} titles (offset {offset})",
                page.len(),
                total
            )?;
        }
        _ => {
            let response = SearchResponse {
                query: query.to_string(),
                total_matches: total,
                returned: page.len(),
                offset,
                has_more: offset + page.len() < total,
                grouped: true,
                results: page,
            };
            writeln!(out, "{}", serde_json::to_string_pretty(&response)?)?;
        }
    }
    Ok(())
}

fn item_row(item: &ItemView) -> String {
    let mut title = item.title.clone();
    if title.chars().count() > 44 {
        title = title.chars().take(43).collect::<String>() + "…";
    }
    format!(
        "{:>7}  {:<44} {:>5}  {:<6} {:<6} {:<7} {:>9}  {:<4} {}",
        item.id,
        title,
        item.year.map(|y| y.to_string()).unwrap_or_default(),
        item.resolution.unwrap_or("-"),
        item.codec.unwrap_or("-"),
        item.source.unwrap_or("-"),
        item.size.as_deref().unwrap_or("-"),
        item.file_type.as_deref().unwrap_or("-"),
        item.category,
    )
}

/// Runs `show` and prints the full record for one id.
pub fn run_show(engine: &SearchEngine, args: &ShowArgs) -> anyhow::Result<()> {
    let item = engine
        .items()
        .iter()
        .find(|item| item.id == args.id)
        .with_context(|| format!("no item with id {}", args.id))?;
    let view = ItemView::new(item, None);
    let mut out = std::io::stdout().lock();
    match args.format.resolve() {
        OutputFormat::Table => {
            writeln!(out, "{}", item_row(&view))?;
            writeln!(out, "file:   {}", item.filename)?;
            writeln!(out, "url:    {}", item.url)?;
            writeln!(out, "folder: {}", item.folder_url)?;
            writeln!(out, "path:   {}", item.path)?;
        }
        OutputFormat::Ids => writeln!(out, "{}", item.id)?,
        _ => {
            let mut value = serde_json::to_value(&view)?;
            value["folder_url"] = item.folder_url.clone().into();
            value["path"] = item.path.clone().into();
            writeln!(out, "{}", serde_json::to_string_pretty(&value)?)?;
        }
    }
    Ok(())
}

/// Filter names accepted by `--category`, in display order.
const FILTER_NAMES: &[&str] = &[
    "All",
    "English",
    "TV Series",
    "Korean",
    "Hindi",
    "Animation",
    "Bangla",
    "Foreign",
    "Chinese/Japanese",
    "3D",
];

/// Runs `categories`: filter names and raw dataset categories with file counts.
pub fn run_categories(engine: &SearchEngine, args: &CategoriesArgs) -> anyhow::Result<()> {
    let filters: Vec<serde_json::Value> = FILTER_NAMES
        .iter()
        .map(|name| {
            let count = engine
                .items()
                .iter()
                .filter(|item| item.matches_category(name))
                .count();
            serde_json::json!({ "name": name, "files": count })
        })
        .collect();
    let mut raw: HashMap<&str, usize> = HashMap::new();
    for item in engine.items() {
        *raw.entry(item.category.as_str()).or_default() += 1;
    }
    let mut raw: Vec<(&str, usize)> = raw.into_iter().collect();
    raw.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let raw: Vec<serde_json::Value> = raw
        .into_iter()
        .map(|(name, files)| serde_json::json!({ "name": name, "files": files }))
        .collect();

    let mut out = std::io::stdout().lock();
    match args.format.resolve() {
        OutputFormat::Table => {
            writeln!(out, "Filters for --category:")?;
            for f in &filters {
                writeln!(
                    out,
                    "  {:<18} {:>7}",
                    f["name"].as_str().unwrap_or(""),
                    f["files"]
                )?;
            }
            writeln!(out, "\nDataset categories (also accepted as substrings):")?;
            for c in &raw {
                writeln!(
                    out,
                    "  {:<36} {:>7}",
                    c["name"].as_str().unwrap_or(""),
                    c["files"]
                )?;
            }
        }
        _ => {
            let value = serde_json::json!({
                "total_files": engine.total_count(),
                "filters": filters,
                "dataset_categories": raw,
                "resolutions": ["4k", "1080p", "720p", "576p", "480p", "360p"],
                "codecs": ["hevc", "x264", "av1"],
                "sources": ["bluray", "web", "hdtv", "dvd", "hdrip", "cam"],
            });
            writeln!(out, "{}", serde_json::to_string_pretty(&value)?)?;
        }
    }
    Ok(())
}

/// Per-id outcome of `download`.
#[derive(Debug, Serialize)]
pub struct DownloadResult {
    pub id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// `completed`, `failed`, `exists`, `not_found` or `dry_run`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Runs `download` and returns the process exit code (0 = everything ok, 1 = any failure).
pub fn run_download(engine: &SearchEngine, args: &DownloadArgs) -> anyhow::Result<i32> {
    let directory = args.output.clone().unwrap_or_else(default_download_dir);

    let wanted: std::collections::HashSet<usize> = args.ids.iter().copied().collect();
    let found: HashMap<usize, &MediaItem> = engine
        .items()
        .iter()
        .filter(|item| wanted.contains(&item.id))
        .map(|item| (item.id, item))
        .collect();

    let mut results: Vec<DownloadResult> = Vec::new();
    let mut queue: VecDeque<(usize, &MediaItem)> = VecDeque::new();
    let mut seen = std::collections::HashSet::new();
    for &id in &args.ids {
        if !seen.insert(id) {
            continue;
        }
        let index = results.len();
        let mut result = DownloadResult {
            id,
            filename: None,
            status: "failed",
            path: None,
            bytes: None,
            size_bytes: None,
            error: None,
        };
        match found.get(&id) {
            None => {
                result.status = "not_found";
                result.error = Some(format!("no item with id {id}"));
            }
            Some(item) if !item.is_file => {
                result.filename = Some(item.filename.clone());
                result.error = Some("item is a directory, not a file".to_string());
            }
            Some(item) => {
                let target = directory.join(&item.filename);
                result.filename = Some(item.filename.clone());
                result.size_bytes = item.size_bytes();
                result.path = Some(target.clone());
                if args.dry_run {
                    result.status = "dry_run";
                } else if args.skip_existing && target.exists() {
                    result.status = "exists";
                } else {
                    queue.push_back((index, item));
                }
            }
        }
        results.push(result);
    }

    if !queue.is_empty() {
        download_queue(args, &directory, queue, &mut results);
    }

    let failed = results
        .iter()
        .any(|r| matches!(r.status, "failed" | "not_found"));
    print_download_report(args, &directory, &results)?;
    Ok(i32::from(failed))
}

fn download_queue(
    args: &DownloadArgs,
    directory: &Path,
    mut queue: VecDeque<(usize, &MediaItem)>,
    results: &mut [DownloadResult],
) {
    let (tx, rx) = channel();
    let jobs = args.jobs as usize;
    let tty = std::io::stderr().is_terminal();
    // task id -> (result index, downloaded, total)
    let mut active: HashMap<u64, (usize, u64, Option<u64>)> = HashMap::new();
    let mut next_task: u64 = 0;
    let mut last_print = Instant::now() - Duration::from_secs(60);
    let interval = if tty {
        Duration::from_millis(250)
    } else {
        Duration::from_secs(5)
    };

    loop {
        while active.len() < jobs
            && let Some((index, item)) = queue.pop_front()
        {
            next_task += 1;
            let total_hint = item.size_bytes();
            match start_download(
                next_task,
                item.url.clone(),
                item.filename.clone(),
                directory.to_path_buf(),
                tx.clone(),
                Arc::new(AtomicBool::new(false)),
                total_hint,
            ) {
                Ok(path) => {
                    results[index].path = Some(path);
                    active.insert(next_task, (index, 0, total_hint));
                }
                Err(error) => {
                    results[index].status = "failed";
                    results[index].error = Some(format!("could not start download: {error}"));
                }
            }
        }
        if active.is_empty() {
            break;
        }

        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(DownloadEvent::Progress {
                id,
                downloaded,
                total,
            }) => {
                if let Some(entry) = active.get_mut(&id) {
                    entry.1 = downloaded;
                    entry.2 = total.or(entry.2);
                }
            }
            Ok(DownloadEvent::Completed {
                id,
                destination,
                downloaded,
            }) => {
                if let Some((index, ..)) = active.remove(&id) {
                    results[index].status = "completed";
                    results[index].path = Some(destination);
                    results[index].bytes = Some(downloaded);
                }
            }
            Ok(DownloadEvent::Failed { id, error }) => {
                if let Some((index, ..)) = active.remove(&id) {
                    results[index].status = "failed";
                    results[index].error = Some(error);
                }
            }
            Ok(DownloadEvent::Cancelled { id, downloaded }) => {
                if let Some((index, ..)) = active.remove(&id) {
                    results[index].status = "failed";
                    results[index].bytes = Some(downloaded);
                    results[index].error = Some("cancelled".to_string());
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if !args.quiet && last_print.elapsed() >= interval && !active.is_empty() {
            last_print = Instant::now();
            let line = active
                .values()
                .map(|(index, done, total)| {
                    let name = results[*index].filename.as_deref().unwrap_or("?");
                    match total {
                        Some(total) if *total > 0 => format!(
                            "{name} {:.1}% ({} / {})",
                            *done as f64 * 100.0 / *total as f64,
                            format_bytes(*done),
                            format_bytes(*total)
                        ),
                        _ => format!("{name} {}", format_bytes(*done)),
                    }
                })
                .collect::<Vec<_>>()
                .join(" | ");
            if tty {
                eprint!("\r\x1b[2K{line}");
            } else {
                eprintln!("downloading: {line}");
            }
        }
    }
    if tty && !args.quiet {
        eprint!("\r\x1b[2K");
    }
}

fn print_download_report(
    args: &DownloadArgs,
    directory: &Path,
    results: &[DownloadResult],
) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    match args.format.resolve() {
        OutputFormat::Table => {
            for r in results {
                writeln!(
                    out,
                    "{:>7}  {:<10} {}{}",
                    r.id,
                    r.status,
                    r.path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .or_else(|| r.filename.clone())
                        .unwrap_or_default(),
                    r.error
                        .as_ref()
                        .map(|e| format!("  ({e})"))
                        .unwrap_or_default()
                )?;
            }
        }
        OutputFormat::Ids => {
            for r in results.iter().filter(|r| r.status == "completed") {
                writeln!(out, "{}", r.id)?;
            }
        }
        OutputFormat::Jsonl => {
            for r in results {
                writeln!(out, "{}", serde_json::to_string(r)?)?;
            }
        }
        _ => {
            let value = serde_json::json!({
                "output_dir": directory,
                "dry_run": args.dry_run,
                "downloads": results,
            });
            writeln!(out, "{}", serde_json::to_string_pretty(&value)?)?;
        }
    }
    Ok(())
}
