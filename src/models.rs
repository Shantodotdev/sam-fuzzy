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
}

impl MediaItem {
    /// Formats a clean human-readable title, appending release year when known.
    ///
    /// # Example
    /// - `Title` + `Some(2024)` -> `"Kraven the Hunter (2024)"`
    /// - `Title` + `None`       -> `"Kraven the Hunter"`
    pub fn display_title(&self) -> String {
        if let Some(y) = self.year {
            format!("{} ({})", self.title, y)
        } else {
            self.title.clone()
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
    ///
    /// Tab matching handles broader groups (e.g., "Foreign" includes Korean, French,
    /// Chinese, etc.), file-type constraints, and case-insensitive fallbacks.
    pub fn matches_category(&self, category_filter: &str) -> bool {
        match category_filter {
            "All" => true,
            "English" => self.category.starts_with("English Movies"),
            "Bangla" => self.category.contains("Bangla"),
            "Korean" => self.category.contains("Korean"),
            "Chinese/Japanese" => {
                self.category.contains("Chinese") || self.category.contains("Japanese")
            }
            "Foreign" => self.category.starts_with("Foreign Language Movies"),
            "3D" => self.category.contains("3D") || self.quality.contains("3D"),
            "Files Only" => self.is_file,
            "Folders Only" => !self.is_file,
            other => self.category.to_lowercase().contains(&other.to_lowercase()),
        }
    }

    /// Returns a short badge tag for display in the results list (e.g. "MKV", "MP4", "DIR").
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
