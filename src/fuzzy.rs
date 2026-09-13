//! High-performance fuzzy search engine for media discovery.
//!
//! Uses a multi-tiered, multi-threaded search architecture:
//! 1. **Character-Presence Bitmask Fast-Reject**: Uses a 64-bit bitmask pre-computed
//!    for each item to reject items lacking the query's characters in sub-nanoseconds.
//! 2. **Parallel SIMD Fuzzy Scoring**: Uses `rayon` to distribute scoring across all CPU
//!    cores in parallel, with per-thread `nucleo-matcher` state to eliminate allocation overhead.
//! 3. **Smart Tiered Ranking**: Applies heavy weight bonuses for exact lowercase titles,
//!    word boundaries, and isolated words.

use crate::models::MediaItem;
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use rayon::prelude::*;

/// A ranked match containing the reference item, composite score, and
/// character indices for UI highlighting.
#[derive(Debug, Clone)]
pub struct SearchResult<'a> {
    pub item: &'a MediaItem,
    pub score: u64,
    pub indices: Vec<u32>,
}

/// Computes a 64-bit character presence bitmask for fast pre-filtering.
///
/// Encodes ASCII 'a'..='z' (bits 0..=25) and '0'..='9' (bits 26..=35).
/// Items whose bitmasks do not contain all query character bits are instantly
/// rejected in a single CPU cycle without invoking the nucleotide fuzzy matcher.
#[inline]
pub fn compute_char_mask(s: &str) -> u64 {
    let mut mask = 0u64;
    for b in s.bytes() {
        match b {
            b'a'..=b'z' => mask |= 1u64 << (b - b'a'),
            b'A'..=b'Z' => mask |= 1u64 << (b - b'A'),
            b'0'..=b'9' => mask |= 1u64 << (26 + b - b'0'),
            _ => {}
        }
    }
    mask
}

/// In-memory search index maintaining pre-lowercased buffers and bitmasks
/// to minimize allocations and maximize throughput during interactive searches.
pub struct SearchEngine {
    items: Vec<MediaItem>,
    haystacks: Vec<String>,
    titles_lower: Vec<String>,
    filenames_lower: Vec<String>,
    char_masks: Vec<u64>,
}

impl SearchEngine {
    /// Initializes the search engine by sorting items in natural ascending order
    /// and pre-caching lowercased titles, filenames, search haystacks, and character bitmasks.
    pub fn new(mut items: Vec<MediaItem>) -> Self {
        items.sort_unstable_by(|a, b| {
            crate::models::natural_cmp(&a.title, &b.title)
                .then_with(|| crate::models::natural_cmp(&a.filename, &b.filename))
        });

        let haystacks: Vec<String> = items.iter().map(|item| item.search_haystack()).collect();
        let titles_lower: Vec<String> =
            items.iter().map(|item| item.title.to_lowercase()).collect();
        let filenames_lower: Vec<String> = items
            .iter()
            .map(|item| item.filename.to_lowercase())
            .collect();
        let char_masks: Vec<u64> = haystacks.iter().map(|h| compute_char_mask(h)).collect();

        Self {
            items,
            haystacks,
            titles_lower,
            filenames_lower,
            char_masks,
        }
    }

    /// Total number of indexed media items.
    pub fn total_count(&self) -> usize {
        self.items.len()
    }

    /// Read-only access to all indexed items.
    pub fn items(&self) -> &[MediaItem] {
        &self.items
    }

    /// Performs parallel fuzzy search across all available CPU cores with tiered ranking.
    pub fn search<'a>(
        &'a self,
        query: &'a str,
        category: &str,
        limit: usize,
    ) -> Vec<SearchResult<'a>> {
        let trimmed_query = query.trim();

        // Fast path: When query is empty, return category items immediately without running fuzzy matcher
        if trimmed_query.is_empty() {
            return self
                .items
                .iter()
                .filter(|item| item.matches_category(category))
                .take(limit)
                .map(|item| SearchResult {
                    item,
                    score: 0,
                    indices: Vec::new(),
                })
                .collect();
        }

        let q_lower = trimmed_query.to_lowercase();
        let q_words: Vec<&str> = q_lower.split_whitespace().collect();
        let query_mask = compute_char_mask(trimmed_query);

        let pattern = Pattern::parse(trimmed_query, CaseMatching::Ignore, Normalization::Smart);

        // Phase 1: Parallel filter and score across all items using Rayon
        let mut matches: Vec<(usize, u64)> = (0..self.items.len())
            .into_par_iter()
            .map_init(
                || (Matcher::new(Config::DEFAULT), Vec::new()),
                |(matcher, utf32_buf), idx| {
                    let item = &self.items[idx];
                    if !item.matches_category(category) {
                        return None;
                    }

                    // Fast-reject pre-filter: check 64-bit character presence bitmask
                    if (self.char_masks[idx] & query_mask) != query_mask {
                        return None;
                    }

                    let haystack = &self.haystacks[idx];
                    let utf32 = Utf32Str::new(haystack, utf32_buf);

                    if let Some(base_score) = pattern.score(utf32, matcher) {
                        let title_lower = &self.titles_lower[idx];
                        let filename_lower = &self.filenames_lower[idx];

                        let mut smart_score: u64 = base_score as u64;

                        // Tier 1: Exact full title match (e.g., query "thor" matches title "Thor" exactly)
                        if title_lower == &q_lower {
                            smart_score += 100_000_000;
                        }
                        // Tier 2: Title starts with the query keyword followed by a word boundary
                        else if title_lower.starts_with(&q_lower) {
                            let next_char = title_lower[q_lower.len()..].chars().next();
                            let is_word_boundary =
                                next_char.map(|c| !c.is_alphanumeric()).unwrap_or(true);
                            if is_word_boundary {
                                smart_score += 50_000_000;
                            } else {
                                smart_score += 5_000_000;
                            }
                            let length_penalty = title_lower.len().min(100) as u64 * 1000;
                            smart_score = smart_score.saturating_sub(length_penalty);
                        }
                        // Tier 3: Query appears as a standalone word anywhere in the title
                        else if title_lower
                            .split(|c: char| !c.is_alphanumeric())
                            .any(|w| w == q_lower)
                        {
                            smart_score += 30_000_000;
                            let length_penalty = title_lower.len().min(100) as u64 * 500;
                            smart_score = smart_score.saturating_sub(length_penalty);
                        }
                        // Tier 4: Multi-word exact phrase match (e.g. "squid game", "dark knight")
                        else if q_words.len() > 1 && title_lower.contains(&q_lower) {
                            smart_score += 25_000_000;
                        }
                        // Tier 5: Multi-word query where all individual words appear in the title
                        else if q_words.len() > 1
                            && q_words.iter().all(|w| {
                                title_lower
                                    .split(|c: char| !c.is_alphanumeric())
                                    .any(|tw| tw == *w)
                            })
                        {
                            smart_score += 20_000_000;
                        }
                        // Tier 6: Substring match inside the title
                        else if title_lower.contains(&q_lower) {
                            smart_score += 10_000_000;
                        }
                        // Tier 7: Standalone word match inside the filename
                        else if filename_lower
                            .split(|c: char| !c.is_alphanumeric())
                            .any(|w| w == q_lower)
                        {
                            smart_score += 2_000_000;
                        }

                        Some((idx, smart_score))
                    } else {
                        None
                    }
                },
            )
            .filter_map(|x| x)
            .collect();

        // Rank by composite score descending, using natural ascending dataset index as tie-breaker
        matches.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Truncate to maximum visible results before computing highlight indices
        if matches.len() > limit {
            matches.truncate(limit);
        }

        // Phase 2: Compute character highlight indices only for the top visible results
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut utf32_buf = Vec::new();
        let mut results = Vec::with_capacity(matches.len());
        for (idx, score) in matches {
            let haystack = &self.haystacks[idx];
            let utf32 = Utf32Str::new(haystack, &mut utf32_buf);
            let mut indices = Vec::new();
            pattern.indices(utf32, &mut matcher, &mut indices);
            results.push(SearchResult {
                item: &self.items[idx],
                score,
                indices,
            });
        }

        results
    }
}
