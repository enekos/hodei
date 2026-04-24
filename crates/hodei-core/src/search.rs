/// JavaScript injected to initialize search highlights and return total match count.
pub const SEARCH_INIT_SCRIPT: &str = r#"(function() {
    const query = arguments[0];
    if (!query) return JSON.stringify({ count: 0 });

    // Remove any existing highlights
    const existing = document.getElementById('__hodei_search_highlights__');
    if (existing) existing.remove();

    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null, false);
    const matches = [];
    let node;
    while (node = walker.nextNode()) {
        const idx = node.textContent.toLowerCase().indexOf(query.toLowerCase());
        if (idx !== -1) {
            matches.push(node);
        }
    }

    if (matches.length === 0) {
        return JSON.stringify({ count: 0 });
    }

    const container = document.createElement('span');
    container.id = '__hodei_search_highlights__';
    container.style.display = 'none';
    document.body.appendChild(container);

    let total = 0;
    for (const textNode of matches) {
        const text = textNode.textContent;
        const parent = textNode.parentNode;
        const queryLower = query.toLowerCase();
        let i = 0;
        while (i < text.length) {
            const idx = text.toLowerCase().indexOf(queryLower, i);
            if (idx === -1) {
                if (i < text.length) {
                    parent.insertBefore(document.createTextNode(text.slice(i)), textNode);
                }
                break;
            }
            if (idx > i) {
                parent.insertBefore(document.createTextNode(text.slice(i, idx)), textNode);
            }
            const mark = document.createElement('mark');
            mark.className = '__hodei_search_match__';
            mark.style.backgroundColor = '#ffcc00';
            mark.style.color = '#000';
            mark.textContent = text.slice(idx, idx + query.length);
            parent.insertBefore(mark, textNode);
            total++;
            i = idx + query.length;
        }
        parent.removeChild(textNode);
    }

    return JSON.stringify({ count: total });
})"#;

/// JavaScript injected to navigate to the next or previous match.
/// Argument: offset (+1 for next, -1 for prev).
pub const SEARCH_NAVIGATE_SCRIPT: &str = r#"(function() {
    const offset = arguments[0];
    const marks = Array.from(document.querySelectorAll('mark.__hodei_search_match__'));
    if (marks.length === 0) return JSON.stringify({ index: 0, count: 0 });

    let current = marks.findIndex(m => m.style.outline === '2px solid red');
    if (current === -1) current = 0;

    if (marks[current]) {
        marks[current].style.outline = '';
    }

    let next = current + offset;
    if (next < 0) next = marks.length - 1;
    if (next >= marks.length) next = 0;

    marks[next].style.outline = '2px solid red';
    marks[next].scrollIntoView({ behavior: 'auto', block: 'center' });

    return JSON.stringify({ index: next + 1, count: marks.length });
})"#;

/// JavaScript injected to clear all search highlights.
pub const SEARCH_CLEAR_SCRIPT: &str = r#"(function() {
    const container = document.getElementById('__hodei_search_highlights__');
    if (container) container.remove();

    const marks = document.querySelectorAll('mark.__hodei_search_match__');
    for (const mark of marks) {
        const parent = mark.parentNode;
        parent.insertBefore(document.createTextNode(mark.textContent), mark);
        parent.removeChild(mark);
        parent.normalize();
    }
    return JSON.stringify({ cleared: true });
})"#;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchResult {
    pub index: usize,
    pub count: usize,
}

impl SearchResult {
    pub fn info_string(&self) -> String {
        if self.count == 0 {
            "0/0".to_string()
        } else {
            format!("{}/{}", self.index, self.count)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_string_no_matches() {
        let r = SearchResult { index: 0, count: 0 };
        assert_eq!(r.info_string(), "0/0");
    }

    #[test]
    fn info_string_with_matches() {
        let r = SearchResult { index: 3, count: 17 };
        assert_eq!(r.info_string(), "3/17");
    }

    #[test]
    fn info_string_ignores_nonzero_index_when_count_is_zero() {
        // Can happen during a race: the JS search counted zero matches after
        // we'd already set an index from a previous query. The display should
        // still read "0/0" not "3/0".
        let r = SearchResult { index: 3, count: 0 };
        assert_eq!(r.info_string(), "0/0");
    }

    #[test]
    fn is_empty_tracks_count() {
        assert!(SearchResult::default().is_empty());
        assert!(SearchResult { index: 0, count: 0 }.is_empty());
        assert!(!SearchResult { index: 1, count: 5 }.is_empty());
    }

    #[test]
    fn equality_requires_both_fields() {
        let a = SearchResult { index: 1, count: 2 };
        let b = SearchResult { index: 1, count: 2 };
        let c = SearchResult { index: 2, count: 2 };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn scripts_are_non_empty_bare_js_functions() {
        // All three constants are bare function expressions; the app-side
        // injector appends `(...)` / `()` at call time. Guard against anyone
        // helpfully self-invoking them here (which would double-call).
        for (name, s) in [
            ("init", SEARCH_INIT_SCRIPT),
            ("navigate", SEARCH_NAVIGATE_SCRIPT),
            ("clear", SEARCH_CLEAR_SCRIPT),
        ] {
            assert!(s.starts_with("(function()"), "{name}: should open with (function()");
            assert!(s.ends_with("})"), "{name}: should end with }}) — got tail {:?}", &s[s.len().saturating_sub(4)..]);
            assert!(s.len() > 50, "{name}: script unexpectedly short");
        }
    }
}
