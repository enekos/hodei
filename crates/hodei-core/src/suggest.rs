#[derive(Debug, Clone, PartialEq)]
pub enum SuggestionSource {
    Bookmark,
    History,
    SearchEngine,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub url: String,
    pub title: String,
    pub source: SuggestionSource,
    pub score: u32,
}

/// Score a candidate against a query. Higher is better.
/// Returns 0 if no match.
pub fn score(query: &str, text: &str) -> u32 {
    if query.is_empty() {
        return 1;
    }
    let q = query.to_lowercase();
    let t = text.to_lowercase();

    if t.starts_with(&q) {
        return 100;
    }

    // Word boundary match
    for word in t.split(|c: char| !c.is_alphanumeric()) {
        if word.starts_with(&q) {
            return 50;
        }
    }

    // Substring match
    if t.contains(&q) {
        return 10;
    }

    0
}

/// Merge and rank suggestions from multiple sources.
/// Bookmarks get a bonus. Results sorted by score descending.
pub fn rank_suggestions(mut suggestions: Vec<Suggestion>, limit: usize) -> Vec<Suggestion> {
    for s in &mut suggestions {
        if s.source == SuggestionSource::Bookmark {
            s.score = s.score.saturating_add(20);
        }
    }
    suggestions.sort_by(|a, b| b.score.cmp(&a.score));
    suggestions.truncate(limit);
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_prefix_scores_highest() {
        assert!(score("rust", "rust-lang.org") > score("rust", "the rust book"));
    }

    #[test]
    fn word_boundary_beats_substring() {
        assert!(score("rust", "learn-rust-fast") > score("rust", "entrusted"));
    }

    #[test]
    fn no_match_scores_zero() {
        assert_eq!(score("xyz", "hello world"), 0);
    }

    #[test]
    fn empty_query_matches_everything() {
        assert!(score("", "anything") > 0);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(score("Rust", "rust-lang.org"), score("rust", "Rust-Lang.org"));
    }

    #[test]
    fn rank_prioritizes_bookmarks() {
        let suggestions = vec![
            Suggestion {
                url: "https://history.com".into(),
                title: "History".into(),
                source: SuggestionSource::History,
                score: 50,
            },
            Suggestion {
                url: "https://bookmark.com".into(),
                title: "Bookmark".into(),
                source: SuggestionSource::Bookmark,
                score: 50,
            },
        ];
        let ranked = rank_suggestions(suggestions, 10);
        assert_eq!(ranked[0].source, SuggestionSource::Bookmark);
    }

    #[test]
    fn rank_truncates_to_limit() {
        let suggestions: Vec<Suggestion> = (0..20)
            .map(|i| Suggestion {
                url: format!("https://{}.com", i),
                title: format!("Site {}", i),
                source: SuggestionSource::History,
                score: i,
            })
            .collect();
        let ranked = rank_suggestions(suggestions, 5);
        assert_eq!(ranked.len(), 5);
    }
}
