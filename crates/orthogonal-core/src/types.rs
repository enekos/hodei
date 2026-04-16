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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}
