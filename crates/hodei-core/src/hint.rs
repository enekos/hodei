use crate::types::HintElement;

const HINT_CHARS: &[u8] = b"asdfghjkl";

/// Generate N labels of uniform length from home-row characters.
/// 1-9 elements: 1 char. 10-81: 2 chars. 82-729: 3 chars.
pub fn generate_labels(count: usize) -> Vec<String> {
    if count == 0 {
        return vec![];
    }
    let base = HINT_CHARS.len();

    let mut length = 1usize;
    while base.pow(length as u32) < count {
        length += 1;
    }

    let mut labels = Vec::with_capacity(count);
    for i in 0..count {
        let mut label = Vec::with_capacity(length);
        let mut n = i;
        for _ in 0..length {
            label.push(HINT_CHARS[n % base] as char);
            n /= base;
        }
        label.reverse();
        labels.push(label.into_iter().collect());
    }
    labels
}

/// Filter labels to those matching the typed prefix.
pub fn filter_labels<'a>(prefix: &str, labels: &'a [String]) -> Vec<&'a String> {
    labels.iter().filter(|l| l.starts_with(prefix)).collect()
}

/// Parse the JSON result from the DOM query script into HintElements.
pub fn parse_hint_elements(json: &str) -> Result<Vec<HintElement>, serde_json::Error> {
    serde_json::from_str(json)
}

/// The JavaScript snippet injected into Servo to find clickable elements.
pub const HINT_QUERY_SCRIPT: &str = r#"(function() {
    const selectors = 'a[href], button, input, select, textarea, [onclick], [role="button"], [role="link"], [tabindex]';
    const elements = document.querySelectorAll(selectors);
    const results = [];
    for (const el of elements) {
        const rect = el.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) continue;
        if (rect.bottom < 0 || rect.top > window.innerHeight) continue;
        results.push({
            tag: el.tagName,
            href: el.href || '',
            text: (el.textContent || '').slice(0, 50),
            x: rect.x + rect.width / 2,
            y: rect.y + rect.height / 2
        });
    }
    return JSON.stringify(results);
})()"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_labels() {
        assert!(generate_labels(0).is_empty());
    }

    #[test]
    fn single_label() {
        let labels = generate_labels(1);
        assert_eq!(labels, vec!["a"]);
    }

    #[test]
    fn nine_labels_are_single_char() {
        let labels = generate_labels(9);
        assert_eq!(labels.len(), 9);
        assert_eq!(labels[0], "a");
        assert_eq!(labels[8], "l");
        assert!(labels.iter().all(|l| l.len() == 1));
    }

    #[test]
    fn ten_labels_are_two_chars() {
        let labels = generate_labels(10);
        assert_eq!(labels.len(), 10);
        assert!(labels.iter().all(|l| l.len() == 2));
        assert_eq!(labels[0], "aa");
        assert_eq!(labels[1], "as");
    }

    #[test]
    fn eighty_one_labels_fills_two_chars() {
        let labels = generate_labels(81);
        assert_eq!(labels.len(), 81);
        assert!(labels.iter().all(|l| l.len() == 2));
        assert_eq!(labels[80], "ll");
    }

    #[test]
    fn eighty_two_labels_are_three_chars() {
        let labels = generate_labels(82);
        assert!(labels.iter().all(|l| l.len() == 3));
    }

    #[test]
    fn labels_are_unique() {
        let labels = generate_labels(81);
        let set: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(set.len(), 81);
    }

    #[test]
    fn filter_labels_by_prefix() {
        let labels = generate_labels(20);
        let filtered = filter_labels("a", &labels);
        assert!(filtered.iter().all(|l| l.starts_with('a')));
        assert!(!filtered.is_empty());
    }

    #[test]
    fn filter_labels_no_match() {
        let labels = generate_labels(5); // single-char: a, s, d, f, g
        let filtered = filter_labels("z", &labels);
        assert!(filtered.is_empty());
    }

    #[test]
    fn parse_hint_elements_from_json() {
        let json = r#"[{"tag":"A","href":"https://example.com","text":"Click me","x":100.5,"y":200.0}]"#;
        let elements = parse_hint_elements(json).unwrap();
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].tag, "A");
        assert_eq!(elements[0].href, "https://example.com");
        assert!((elements[0].x - 100.5).abs() < 0.01);
    }

    #[test]
    fn parse_hint_elements_empty() {
        let elements = parse_hint_elements("[]").unwrap();
        assert!(elements.is_empty());
    }

    #[test]
    fn three_char_labels_appear_for_large_counts() {
        let labels = generate_labels(100);
        assert_eq!(labels.len(), 100);
        assert!(labels.iter().all(|l| l.len() == 3));
    }

    #[test]
    fn labels_use_only_home_row_chars() {
        // Any non-home-row char would make hints unreachable.
        let home = "asdfghjkl";
        for l in generate_labels(200) {
            for c in l.chars() {
                assert!(home.contains(c), "label {l:?} contains off-row char {c:?}");
            }
        }
    }

    #[test]
    fn filter_with_full_label_matches_exactly_one() {
        let labels = generate_labels(20);
        let target = labels[5].clone();
        let filtered = filter_labels(&target, &labels);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], &target);
    }

    #[test]
    fn filter_with_empty_prefix_matches_all() {
        let labels = generate_labels(9);
        assert_eq!(filter_labels("", &labels).len(), 9);
    }

    #[test]
    fn parse_hint_elements_malformed_returns_err() {
        assert!(parse_hint_elements("{not json").is_err());
        assert!(parse_hint_elements("[{\"tag\":5}]").is_err()); // tag must be string
    }

    #[test]
    fn query_script_has_selector_coverage() {
        // Changing this script is a cross-cutting change; pin the selectors
        // Hodei relies on. If you intentionally drop one, update the test.
        for needle in ["a[href]", "button", "input", "[role=\"button\"]", "tabindex"] {
            assert!(HINT_QUERY_SCRIPT.contains(needle), "missing selector {needle}");
        }
    }
}
