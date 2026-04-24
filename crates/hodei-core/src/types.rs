// === Identity ===

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewId(pub u64);

// === Geometry ===

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    pub fn right(&self) -> f32 { self.x + self.width }
    pub fn bottom(&self) -> f32 { self.y + self.height }
}

// === Directions ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

// === Input types (core-owned, no winit/servo dependency) ===

#[derive(Debug, Clone, PartialEq)]
pub struct CoreKeyEvent {
    pub key: CoreKey,
    pub state: KeyState,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    /// True when no modifier is held — useful for distinguishing a bare
    /// keypress from a chord.
    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.shift && !self.alt && !self.meta
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoreKey {
    Char(char),
    Escape,
    Enter,
    Backspace,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoreMouseEvent {
    Move { x: f32, y: f32 },
    Down { x: f32, y: f32, button: MouseButton },
    Up { x: f32, y: f32, button: MouseButton },
    Scroll { x: f32, y: f32, delta_x: f32, delta_y: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoreInputEvent {
    Key(CoreKeyEvent),
    Mouse(CoreMouseEvent),
}

// === Tile state (facade-compatible, no Servo types) ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileLoadStatus {
    Started,
    HeadParsed,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileCursor {
    Default,
    Pointer,
    Text,
}

// === Hint types ===

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct HintElement {
    pub tag: String,
    pub href: String,
    pub text: String,
    pub x: f64,
    pub y: f64,
}

// === Session types ===

#[derive(Debug, Clone, PartialEq)]
pub struct SessionInfo {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub tile_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNodeRow {
    pub node_index: u32,
    pub is_leaf: bool,
    pub direction: Option<SplitDirection>,
    pub ratio: Option<f32>,
    pub view_id: Option<ViewId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TileRow {
    pub view_id: ViewId,
    pub url: String,
    pub title: String,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

// === Metadata events (from Servo delegate back to app) ===

#[derive(Debug, Clone, PartialEq)]
pub enum MetadataEvent {
    UrlChanged { view_id: ViewId, url: String },
    TitleChanged { view_id: ViewId, title: String },
    StatusTextChanged { view_id: ViewId, text: Option<String> },
    FrameReady { view_id: ViewId },
    LoadStatusChanged { view_id: ViewId, status: TileLoadStatus },
    HistoryChanged { view_id: ViewId, can_back: bool, can_forward: bool },
    DevToolsStarted { port: u16, token: String },
    DevToolsConnectionRequest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_id_equality() {
        assert_eq!(ViewId(1), ViewId(1));
        assert_ne!(ViewId(1), ViewId(2));
    }

    #[test]
    fn rect_contains_point() {
        let r = Rect { x: 10.0, y: 20.0, width: 100.0, height: 50.0 };
        assert!(r.contains(50.0, 40.0));
        assert!(!r.contains(5.0, 40.0));
        assert!(!r.contains(50.0, 80.0));
    }

    #[test]
    fn rect_default_is_zero() {
        let r = Rect::default();
        assert_eq!(r.x, 0.0);
        assert_eq!(r.width, 0.0);
    }

    #[test]
    fn modifiers_default_is_none() {
        let m = Modifiers::default();
        assert!(!m.ctrl);
        assert!(!m.shift);
        assert!(!m.alt);
        assert!(!m.meta);
    }

    #[test]
    fn rect_contains_is_half_open() {
        // Left/top edges inside, right/bottom edges outside — this matches the
        // expectation that tiling rects tile without double-counting shared
        // borders.
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(10.0, 20.0));            // top-left corner: in
        assert!(!r.contains(110.0, 40.0));          // right edge: out
        assert!(!r.contains(50.0, 70.0));           // bottom edge: out
        assert!(!r.contains(9.9999, 40.0));         // just-left: out
    }

    #[test]
    fn rect_is_empty_handles_zero_and_negative() {
        assert!(Rect::default().is_empty());
        assert!(Rect::new(0.0, 0.0, 0.0, 10.0).is_empty());
        assert!(Rect::new(0.0, 0.0, 10.0, 0.0).is_empty());
        assert!(Rect::new(0.0, 0.0, -1.0, 10.0).is_empty());
        assert!(!Rect::new(0.0, 0.0, 10.0, 10.0).is_empty());
    }

    #[test]
    fn rect_right_bottom() {
        let r = Rect::new(5.0, 10.0, 100.0, 200.0);
        assert_eq!(r.right(), 105.0);
        assert_eq!(r.bottom(), 210.0);
    }

    #[test]
    fn modifiers_is_empty() {
        assert!(Modifiers::default().is_empty());
        let m = Modifiers { ctrl: true, ..Default::default() };
        assert!(!m.is_empty());
    }

    #[test]
    fn direction_is_copyable_and_equatable() {
        let a = Direction::Up;
        let b = a;                     // Copy
        assert_eq!(a, b);              // PartialEq
        assert_ne!(Direction::Up, Direction::Down);
    }

    #[test]
    fn split_direction_roundtrip_through_storage() {
        // Simulates the session-serialization path ("h"/"v" strings).
        for d in [SplitDirection::Horizontal, SplitDirection::Vertical] {
            let s = match d {
                SplitDirection::Horizontal => "h",
                SplitDirection::Vertical => "v",
            };
            let back = match s {
                "h" => SplitDirection::Horizontal,
                "v" => SplitDirection::Vertical,
                _ => unreachable!(),
            };
            assert_eq!(d, back);
        }
    }

    #[test]
    fn core_key_event_equality() {
        let a = CoreKeyEvent {
            key: CoreKey::Char('x'),
            state: KeyState::Pressed,
            modifiers: Modifiers::default(),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn mouse_event_move_is_distinct_from_down() {
        let m = CoreMouseEvent::Move { x: 1.0, y: 2.0 };
        let d = CoreMouseEvent::Down { x: 1.0, y: 2.0, button: MouseButton::Left };
        assert_ne!(m, d);
    }

    #[test]
    fn view_id_is_hashable() {
        let mut set = std::collections::HashSet::new();
        set.insert(ViewId(1));
        set.insert(ViewId(1));
        set.insert(ViewId(2));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn tile_load_status_progression_is_distinct() {
        assert_ne!(TileLoadStatus::Started, TileLoadStatus::HeadParsed);
        assert_ne!(TileLoadStatus::HeadParsed, TileLoadStatus::Complete);
    }

    #[test]
    fn hint_element_deserialises() {
        let json = r#"{"tag":"BUTTON","href":"","text":"OK","x":42.0,"y":7.5}"#;
        let e: HintElement = serde_json::from_str(json).unwrap();
        assert_eq!(e.tag, "BUTTON");
        assert!(e.href.is_empty());
    }
}
