//! Lucide icon font codepoints (pinned to lucide-static 1.8.0).
//!
//! These map Lucide icon names to the Private-Use-Area codepoints used by
//! `assets/fonts/lucide.ttf`. Regenerate when bumping the font version.
//!
//! The font family name inside the TTF is `"lucide"` (lowercase).
pub const FONT_FAMILY: &str = "lucide";

pub const ARROW_LEFT: &str = "\u{e048}";
pub const ARROW_RIGHT: &str = "\u{e049}";
pub const ARROW_LEFT_RIGHT: &str = "\u{e24a}";
pub const ROTATE_CW: &str = "\u{e149}";
pub const HOUSE: &str = "\u{e0f5}";
pub const COLUMNS_2: &str = "\u{e098}";
pub const ROWS_2: &str = "\u{e439}";
pub const X: &str = "\u{e1b2}";
pub const TARGET: &str = "\u{e180}";
pub const SEARCH: &str = "\u{e151}";
pub const TERMINAL: &str = "\u{e181}";
pub const BOOKMARK: &str = "\u{e060}";
pub const LOCK: &str = "\u{e10b}";
pub const LOADER: &str = "\u{e109}";
pub const STAR: &str = "\u{e176}";
pub const ZOOM_IN: &str = "\u{e1b6}";

#[cfg(test)]
mod tests {
    use super::*;

    /// All icons Hodei uses in the HUD.
    const ALL: &[(&str, &str)] = &[
        ("ARROW_LEFT", ARROW_LEFT),
        ("ARROW_RIGHT", ARROW_RIGHT),
        ("ARROW_LEFT_RIGHT", ARROW_LEFT_RIGHT),
        ("ROTATE_CW", ROTATE_CW),
        ("HOUSE", HOUSE),
        ("COLUMNS_2", COLUMNS_2),
        ("ROWS_2", ROWS_2),
        ("X", X),
        ("TARGET", TARGET),
        ("SEARCH", SEARCH),
        ("TERMINAL", TERMINAL),
        ("BOOKMARK", BOOKMARK),
        ("LOCK", LOCK),
        ("LOADER", LOADER),
        ("STAR", STAR),
        ("ZOOM_IN", ZOOM_IN),
    ];

    #[test]
    fn font_family_matches_slint_binding() {
        // The Slint side literally uses this string; keep them in sync.
        assert_eq!(FONT_FAMILY, "lucide");
    }

    #[test]
    fn every_glyph_is_a_single_pua_codepoint() {
        // Lucide packs all glyphs into the Basic-Multilingual-Plane PUA (E000-F8FF).
        for (name, glyph) in ALL {
            let chars: Vec<char> = glyph.chars().collect();
            assert_eq!(chars.len(), 1, "{name}: expected single char, got {:?}", chars);
            let c = chars[0] as u32;
            assert!(
                (0xE000..=0xF8FF).contains(&c),
                "{name}: U+{c:04X} is outside the BMP Private Use Area"
            );
        }
    }

    #[test]
    fn every_icon_codepoint_is_unique() {
        // Guards against a copy-paste slip that would make two icons alias.
        let mut seen = std::collections::HashMap::new();
        for (name, glyph) in ALL {
            if let Some(prev) = seen.insert(*glyph, *name) {
                panic!("{name} and {prev} share glyph {glyph:?}");
            }
        }
    }
}
