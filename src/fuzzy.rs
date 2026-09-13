//! High-performance fuzzy search engine for media discovery.
//!
//! Uses a two-phase ranking architecture:
//! 1. **Phase 1 (Broad Match)**: `nucleo-matcher` checks if the query matches the
//!    pre-computed haystack string using bitwise and SIMD-accelerated fuzzy matching.
//! 2. **Phase 2 (Smart Bonus Tiering)**: Applies heavy weight multipliers to exact
//!    lowercase title matches, prefix boundaries, and isolated words. This guarantees
//!    that searching for `"thor"` or `"squid game"` prioritizes the intended title
//!    over coincidental fuzzy substrings (e.g. "The Paradise of Thorns").

use crate::models::MediaItem;
use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};

/// A ranked match containing the reference item, composite score, and
/// character indices for UI highlighting.
#[derive(Debug, Clone)]
pub struct SearchResult<'a> {
    pub item: &'a MediaItem,
    pub score: u64,
    pub indices: Vec<u32>,
}

/// In-memory search index maintaining pre-lowercased buffers to minimize
/// allocations during keystroke-driven interactive searches.
pub struct SearchEngine {
    items: Vec<MediaItem>,
    haystacks: Vec<String>,
    titles_lower: Vec<String>,
    filenames_lower: Vec<String>,
}

impl SearchEngine {
    /// Initializes the search engine by sorting items in natural ascending order
    /// and pre-caching lowercased titles, filenames, and search haystacks.
    pub fn new(mut items: Vec<MediaItem>) -> Self {
        items.sort_unstable_by(|a, b| {
            crate::models::natural_cmp(&a.title, &b.title)
                .then_with(|| crate::models::natural_cmp(&a.filename, &b.filename))
        });

        let haystacks = items.iter().map(|item| item.search_haystack()).collect();
        let titles_lower = items.iter().map(|item| item.title.to_lowercase()).collect();
        let filenames_lower = items.iter().map(|item| item.filename.to_lowercase()).collect();
        Self {
            items,
            haystacks,
            titles_lower,
            filenames_lower,
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

    /// Performs fuzzy search and applies tiered ranking.
    ///
    /// # Performance
    /// - Reuses UTF-32 conversion buffers across items.
    /// - Computes character highlight indices *only* for the top `limit` results
    ///   after sorting, avoiding tens of thousands of temporary vector allocations.
    pub fn search<'a>(&'a self, query: &'a str, category: &str, limit: usize) -> Vec<SearchResult<'a>> {
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

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(trimmed_query, CaseMatching::Ignore, Normalization::Smart);

        let mut utf32_buf = Vec::new();
        let mut matches: Vec<(usize, u64)> = Vec::new();

        // Phase 1: Filter and compute composite scores
        for (idx, item) in self.items.iter().enumerate() {
            if !item.matches_category(category) {
                continue;
            }

            let haystack = &self.haystacks[idx];
            let utf32 = Utf32Str::new(haystack, &mut utf32_buf);

            if let Some(base_score) = pattern.score(utf32, &mut matcher) {
                let title_lower = &self.titles_lower[idx];
                let filename_lower = &self.filenames_lower[idx];

                let mut smart_score: u64 = base_score as u64;

                // Tier 1: Exact full title match (e.g., query "thor" matches title "Thor" exactly)
                if title_lower == &q_lower {
                    smart_score += 100_000_000;
                }
                // Tier 2: Title starts with the query keyword followed by a word boundary
                // (e.g. "Thor (2011)" or "Thor-Love and Thunder")
                else if title_lower.starts_with(&q_lower) {
                    let next_char = title_lower[q_lower.len()..].chars().next();
                    let is_word_boundary = next_char.map(|c| !c.is_alphanumeric()).unwrap_or(true);
                    if is_word_boundary {
                        smart_score += 50_000_000;
                    } else {
                        // Starts with prefix but not whole word (e.g. "thorns" when querying "thor")
                        smart_score += 5_000_000;
                    }
                    // Shorter, cleaner titles rank higher (e.g. standalone "Thor" beats "Thor-Love and Thunder")
                    let length_penalty = title_lower.len().min(100) as u64 * 1000;
                    smart_score = smart_score.saturating_sub(length_penalty);
                }
                // Tier 3: Query appears as a standalone word anywhere in the title
                // (e.g. "Return of Thor" matching "thor")
                else if title_lower.split(|c: char| !c.is_alphanumeric()).any(|w| w == q_lower) {
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
                    && q_words.iter().all(|w| title_lower.split(|c: char| !c.is_alphanumeric()).any(|tw| tw == *w))
                {
                    smart_score += 20_000_000;
                }
                // Tier 6: Substring match inside the title
                else if title_lower.contains(&q_lower) {
                    smart_score += 10_000_000;
                }
                // Tier 7: Standalone word match inside the filename
                else if filename_lower.split(|c: char| !c.is_alphanumeric()).any(|w| w == q_lower) {
                    smart_score += 2_000_000;
                }

                matches.push((idx, smart_score));
            }
        }

        // Rank by composite score descending, using natural ascending dataset index as tie-breaker
        matches.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Truncate to maximum visible results before computing highlight indices
        if matches.len() > limit {
            matches.truncate(limit);
        }

        // Phase 2: Compute character highlight indices only for the top visible results
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
