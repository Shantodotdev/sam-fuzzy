//! Data models and dataset loaders for SamOnline media items.
//!
//! Handles serialization, title formatting, category classification, and
//! preparing search haystacks for high-speed indexing.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Represents a single media entry (movie, series season, episode, or folder)
/// crawled from SamOnline FTP mirrors (DhakaFlix).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaItem {
    /// Unique incremental identifier within the local index.
    pub id: usize,

    /// Cleaned media title (e.g., "Thor", "Squid Game", "Inception").
    pub title: String,

    /// Release year if parsed from directory or file name.
    pub year: Option<u16>,

    /// Quality or source profile (e.g., "1080p BluRay", "720p WEBRip", "Dual Audio").
    pub quality: String,

    /// Category path (e.g., "English Movies", "Foreign Language Movies / Korean Series").
    pub category: String,

    /// Exact file name or leaf directory name on the server.
    pub filename: String,

    /// True if pointing directly to a media file (.mkv, .mp4), false if a directory.
    pub is_file: bool,

    /// Direct HTTP stream or download URL on the SamOnline mirror.
    pub url: String,

    /// URL to the parent directory on the mirror (opens the folder in h5ai).
    pub folder_url: String,

    /// Server identifier (e.g., "DHAKA-FLIX-7", "DHAKA-FLIX-14", "DHAKA-FLIX-12").
    pub server: String,

    /// Relative path hierarchy as extracted from the crawl.
    pub path: String,

    /// Human-readable file size if available (e.g., "954.1 MB", "1.42 GB").
    #[serde(default)]
    pub size: Option<String>,
}

impl MediaItem {
    /// Formats a clean human-readable title, stripping trailing file extensions
    /// and appending release year when known.
    pub fn display_title(&self) -> String {
        let mut clean_title = self.title.clone();
        for ext in &[".mkv", ".mp4", ".avi", ".webm", ".flv", " -mkvC", "-mkvC"] {
            if clean_title.to_lowercase().ends_with(&ext.to_lowercase()) {
                clean_title.truncate(clean_title.len() - ext.len());
            }
        }
        if let Some(y) = self.year {
            format!("{} ({})", clean_title, y)
        } else {
            clean_title
        }
    }

    /// Combines title, year, category, quality, filename, and server into a single
    /// searchable string. This allows fuzzy searches like "witcher 2025" or "batman 1080p"
    /// to match across multiple metadata fields in one pass.
    pub fn search_haystack(&self) -> String {
        let year_str = self.year.map(|y| y.to_string()).unwrap_or_default();
        format!(
            "{} {} {} {} {} {}",
            self.title, year_str, self.category, self.quality, self.filename, self.server
        )
    }

    /// Evaluates whether this item satisfies the active category filter tab.
    pub fn matches_category(&self, category_filter: &str) -> bool {
        match category_filter {
            "All" => true,
            "English" => self.category.starts_with("English Movies"),
            "TV Series" => self.category.contains("TV Series"),
            "Korean" => self.category.contains("Korean"),
            "Hindi" => self.category.contains("Hindi") || self.category.contains("South Indian"),
            "Animation" => self.category.contains("Animation"),
            "Bangla" => self.category.contains("Bangla"),
            "Foreign" => self.category.starts_with("Foreign Language Movies") || self.category.contains("Korean"),
            "Chinese/Japanese" => {
                self.category.contains("Chinese") || self.category.contains("Japanese")
            }
            "3D" => self.category.contains("3D") || self.quality.contains("3D"),
            "Files Only" => self.is_file,
            "Folders Only" => !self.is_file,
            other => self.category.to_lowercase().contains(&other.to_lowercase()),
        }
    }

    /// Returns a short badge tag for display (e.g. "MKV", "MP4", "DIR").
    pub fn file_type_label(&self) -> &'static str {
        if self.is_file {
            let lower = self.filename.to_lowercase();
            if lower.ends_with(".mkv") {
                "MKV"
            } else if lower.ends_with(".mp4") {
                "MP4"
            } else if lower.ends_with(".avi") {
                "AVI"
            } else {
                "FILE"
            }
        } else {
            "DIR"
        }
    }

    /// Extracts a normalized resolution label (e.g. "4K", "1080p", "720p", "576p", "480p", "360p")
    /// from the item's quality profile or physical filename.
    pub fn clean_resolution(&self) -> Option<&'static str> {
        let q_lower = self.quality.to_lowercase();
        let f_lower = self.filename.to_lowercase();

        if q_lower.contains("2160")
            || q_lower.contains("4k")
            || q_lower.contains("uhd")
            || f_lower.contains("2160")
            || f_lower.contains("4k")
            || f_lower.contains("uhd")
        {
            Some("4K")
        } else if q_lower.contains("1080") || f_lower.contains("1080") {
            Some("1080p")
        } else if q_lower.contains("720") || f_lower.contains("720") {
            Some("720p")
        } else if q_lower.contains("576") || f_lower.contains("576") {
            Some("576p")
        } else if q_lower.contains("480") || f_lower.contains("480") {
            Some("480p")
        } else if q_lower.contains("360") || f_lower.contains("360") {
            Some("360p")
        } else {
            None
        }
    }
}

/// Natural alphanumeric ordering comparator (e.g. "S01E02" < "S01E10").
/// Compares consecutive digit sequences as integers so multi-season series
/// are naturally ordered for intuitive tracking.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut num_a: u64 = 0;
                while let Some(d) = a_chars.peek() {
                    if let Some(val) = d.to_digit(10) {
                        num_a = num_a.saturating_mul(10).saturating_add(val as u64);
                        a_chars.next();
                    } else {
                        break;
                    }
                }
                let mut num_b: u64 = 0;
                while let Some(d) = b_chars.peek() {
                    if let Some(val) = d.to_digit(10) {
                        num_b = num_b.saturating_mul(10).saturating_add(val as u64);
                        b_chars.next();
                    } else {
                        break;
                    }
                }
                match num_a.cmp(&num_b) {
                    std::cmp::Ordering::Equal => continue,
                    ord => return ord,
                }
            }
            (Some(ca), Some(cb)) => {
                let la = ca.to_ascii_lowercase();
                let lb = cb.to_ascii_lowercase();
                match la.cmp(&lb) {
                    std::cmp::Ordering::Equal => {
                        a_chars.next();
                        b_chars.next();
                    }
                    ord => return ord,
                }
            }
        }
    }
}

/// Reads and deserializes the media dataset from a JSON file.
///
/// Uses buffered I/O to parse large datasets (50k+ items) in ~200ms.
pub fn load_dataset<P: AsRef<Path>>(path: P) -> anyhow::Result<Vec<MediaItem>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let items: Vec<MediaItem> = serde_json::from_reader(reader)?;
    Ok(items)
}

/// Parses media dataset from an in-memory JSON string (primarily used in unit tests).
pub fn load_dataset_from_str(json_str: &str) -> anyhow::Result<Vec<MediaItem>> {
    let items: Vec<MediaItem> = serde_json::from_str(json_str)?;
    Ok(items)
}
