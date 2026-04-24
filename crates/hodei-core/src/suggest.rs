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

    #[test]
    fn rank_empty_returns_empty() {
        let ranked = rank_suggestions(vec![], 10);
        assert!(ranked.is_empty());
    }

    #[test]
    fn rank_limit_zero_returns_empty() {
        let s = vec![Suggestion {
            url: "a".into(), title: "A".into(),
            source: SuggestionSource::History, score: 100,
        }];
        assert!(rank_suggestions(s, 0).is_empty());
    }

    #[test]
    fn rank_limit_larger_than_len_returns_all() {
        let s = vec![
            Suggestion { url: "a".into(), title: "A".into(), source: SuggestionSource::History, score: 1 },
            Suggestion { url: "b".into(), title: "B".into(), source: SuggestionSource::History, score: 2 },
        ];
        assert_eq!(rank_suggestions(s, 50).len(), 2);
    }

    #[test]
    fn rank_bookmark_bonus_can_overtake_higher_history() {
        // History at 40 vs bookmark at 25+20=45 → bookmark wins.
        let s = vec![
            Suggestion { url: "h".into(), title: "H".into(), source: SuggestionSource::History, score: 40 },
            Suggestion { url: "b".into(), title: "B".into(), source: SuggestionSource::Bookmark, score: 25 },
        ];
        let ranked = rank_suggestions(s, 10);
        assert_eq!(ranked[0].source, SuggestionSource::Bookmark);
    }

    #[test]
    fn rank_saturates_at_u32_max_for_bookmark_bonus() {
        // The saturating_add guard means we don't overflow even at the cap.
        let s = vec![Suggestion {
            url: "b".into(), title: "B".into(),
            source: SuggestionSource::Bookmark,
            score: u32::MAX,
        }];
        let ranked = rank_suggestions(s, 10);
        assert_eq!(ranked[0].score, u32::MAX);
    }

    #[test]
    fn score_prefix_longer_than_text_is_no_match() {
        assert_eq!(score("rustlang-forever", "rust"), 0);
    }

    #[test]
    fn score_unicode_substring() {
        // Lowercasing non-ASCII should still match.
        assert!(score("café", "Café Luna") > 0);
    }
}
