# Hodei Browser — v0.1.0 Design Spec

## Overview

Hodei is a keyboard-driven tiling web browser built on the Servo engine. It targets power users who want vim-style modal navigation, BSP tiling window management, and zero mouse dependency.

**Core stack:** Servo (rendering), Slint (HUD overlay), Winit (windowing), SQLite/rusqlite (persistence), glow (GL compositing).

**Version scope:** v0.1.0 delivers all four subsystems: project scaffolding with clean module separation, Servo integration via the modern delegate API, session persistence with autosave, and hint mode for keyboard-driven link navigation.

---

## 1. Module Structure & Crate Layout

```
hodei/
├── Cargo.toml                        # workspace root
├── crates/
│   ├── hodei-app/               # binary crate
│   │   └── src/
│   │       ├── main.rs               # CLI args, init logging, launch app
│   │       └── app.rs                # Winit event loop, top-level orchestration
│   ├── hodei-core/              # library crate — all browser logic
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── view.rs               # WebView wrapper (one per tile)
│   │       ├── layout.rs             # LayoutManager trait + BSP Tiling impl
│   │       ├── input.rs              # Modal input state machine
│   │       ├── compositor.rs         # GL compositor
│   │       ├── hint.rs               # Hint mode logic
│   │       ├── session.rs            # Session persistence (SQLite)
│   │       └── hud.rs                # Slint HUD bridge
│   └── hodei-servo/             # library crate — Servo facade
│       └── src/
│           ├── lib.rs
│           ├── delegate.rs           # ServoDelegate + WebViewDelegate impls
│           ├── context.rs            # RenderingContext management
│           └── events.rs             # Winit → Servo event translation
├── ui/
│   └── hud.slint                     # Slint UI definitions
├── migrations/
│   └── 001_init.sql                  # SQLite schema
├── servo/                            # git submodule (servo/servo)
└── ladybird/                         # git submodule (LadybirdBrowser/ladybird, reference only)
```

### Rationale

Three crates with clear dependency direction:

- `hodei-servo` — wraps all Servo API surface. When Servo's internals change, only this crate updates. Exports clean types (`Engine`, `Tile`, `TileId`, `EngineEvents`).
- `hodei-core` — all browser logic (layout, input, compositing, hints, sessions, HUD). Depends on `hodei-servo` but never imports Servo crates directly. Testable with mock facades. The `view.rs` module holds per-tile application state (`ViewId`, URL, title, dirty flag) and orchestrates calls to the facade — it is the bridge between the layout engine and the servo facade.
- `hodei-app` — binary entrypoint only. Depends on both `core` and `servo`. Initializes Winit, constructs the `App`, and runs the event loop.

### Git Submodules

- `servo/` — pinned to a known-good commit. Built from source. The `hodei-servo` facade crate depends on `servo` (the `libservo` crate) from this path.
- `ladybird/` — reference only. Not compiled, not linked. Used for architectural inspiration (particularly `LibWebView::ViewImplementation` pattern and IPC boundary design).

---

## 2. Event Loop & Data Flow

Winit's `EventLoop::run()` is the single top-level loop. Servo runs its work on background threads and signals the embedder via `EventLoopWaker`.

### Flow

```
Winit EventLoop
│
├─ WindowEvent::KeyboardInput / MouseInput
│   → InputRouter.handle(event) → Vec<Action>
│   → App dispatches each Action:
│       ├─ FocusNeighbor/Split/Close → LayoutManager mutation
│       ├─ ForwardToServo → engine.send_input(view_id, event)
│       ├─ Navigate → engine.navigate(view_id, url)
│       ├─ EnterHintMode → engine.evaluate_js(...) → hint flow
│       └─ EnterInsert/Command/ExitToNormal → mode change + HUD update
│
├─ WindowEvent::Resized
│   → LayoutManager.resize(new_size)
│   → BSP tree recalculates all tile rects
│   → For each tile: engine.resize_tile(view_id, new_rect)
│   → Compositor.resize(new_size)
│   → Hud.resize(new_size)
│
├─ WindowEvent::RedrawRequested
│   → Compositor.draw(tiles, hud_buffer)
│       1. glClear
│       2. For each (rect, texture) in resolved tiles: blit FBO
│       3. glEnable(GL_BLEND), upload Slint buffer, draw overlay
│       4. SwapBuffers
│
└─ UserEvent::ServoTick (from EventLoopWaker)
    → servo.spin_event_loop()
    → WebViewDelegate callbacks fire:
        ├─ notify_new_frame_ready → mark tile dirty, request_redraw()
        ├─ notify_url_changed → update TileState, update HUD
        ├─ notify_title_changed → update TileState, update HUD
        └─ notify_load_status_changed → update TileState
```

### Servo Thread Wakeup

`EventLoopWaker` wraps Winit's `EventLoopProxy<UserEvent>`. When Servo's background threads need attention, they call `wake()`, which sends `UserEvent::ServoTick`. The Winit handler calls `servo.spin_event_loop()`, which processes all pending delegate callbacks synchronously on the main thread.

### Input Routing

All keyboard and mouse events hit `InputRouter` first. Based on current mode:

- **Normal mode:** Single keystrokes map to `Action`s. Keys are consumed, never forwarded to Servo.
- **Insert mode:** All keys forward to Servo via `engine.send_input()` except `Esc` (returns to Normal).
- **Command mode:** Keystrokes build a command buffer displayed in the Slint command bar. `Enter` parses and executes. `Esc` cancels.
- **Hint mode:** Keystrokes filter 2-letter labels. Match triggers a click. `Esc` cancels.

### Resize Cascade

Window resize → `LayoutManager.resize(new_size)` → BSP tree recalculates all tile rectangles → each `View.resize(new_rect)` → `OffscreenRenderingContext.resize()` + `WebView.resize()`.

---

## 3. Servo Facade (`hodei-servo`)

This crate is the sole interface between Hodei and Servo. `hodei-core` never imports Servo crates.

### ID Convention

A single `ViewId` type (a `u64` newtype) is used everywhere — `hodei-core` layout, compositor, session, and the servo facade all share this type. It is defined in `hodei-core` and re-used by `hodei-servo`.

### Public Types

```rust
// Re-exported wrapper types (no Servo imports leak to core)
pub use hodei_core::ViewId;

pub struct Engine {
    servo: Servo,
    window_context: WindowRenderingContext,
}

pub struct Tile {
    webview: WebView,
    offscreen_context: OffscreenRenderingContext,
    state: TileState,
}

/// Facade-owned type — does NOT re-export Servo's types.
/// hodei-core depends on this, not on Servo crates.
pub struct TileState {
    pub url: String,               // plain string, not servo::url::Url
    pub title: String,
    pub load_status: TileLoadStatus,  // facade-defined enum
    pub cursor: TileCursor,           // facade-defined enum
}

pub enum TileLoadStatus { Started, HeadParsed, Complete }
pub enum TileCursor { Default, Pointer, Text, /* minimal set for v0.1.0 */ }

pub trait EngineEvents {
    fn on_new_frame(&mut self, view_id: ViewId);
    fn on_url_changed(&mut self, view_id: ViewId, url: String);
    fn on_title_changed(&mut self, view_id: ViewId, title: String);
    fn on_load_status_changed(&mut self, view_id: ViewId, status: TileLoadStatus);
    fn on_cursor_changed(&mut self, view_id: ViewId, cursor: TileCursor);
}
```

### Engine Methods

- `new(display_handle, window_handle, size) -> Engine` — creates `WindowRenderingContext`, builds `Servo` via `ServoBuilder`, sets up `ServoDelegate`.
- `create_tile(url: &str) -> ViewId` — creates `OffscreenRenderingContext` + `WebView` via `WebViewBuilder`. Registers `WebViewDelegate` routing callbacks through `EngineEvents`.
- `destroy_tile(view_id)` — drops `WebView` and its offscreen context.
- `resize_tile(view_id, rect)` — calls `offscreen_context.resize()` + `webview.resize()`.
- `paint_tile(view_id)` — calls `webview.paint()`. FBO texture is ready to read afterward.
- `tile_texture(view_id) -> GLuint` — returns the FBO's color attachment texture ID.
- `send_input(view_id, event: CoreInputEvent)` — translates `CoreInputEvent` (defined in `hodei-core`) to Servo's `InputEvent` and forwards to `webview.notify_input_event()`.
- `navigate(view_id, url: &str)` — parses URL and calls `webview.load(url)`.
- `go_back(view_id)` / `go_forward(view_id)` — history navigation.
- `evaluate_js(view_id, script: &str, callback)` — for hint mode DOM queries.
- `spin()` — calls `servo.spin_event_loop()`, processes delegate callbacks.

### Delegate Internals

`ServoDelegate` handles engine-level events (errors, devtools). Most methods are no-ops for v0.1.0.

`WebViewDelegate` — one instance per Tile. Routes callbacks to `EngineEvents`:
- `notify_new_frame_ready()` → `events.on_new_frame(view_id)` + `waker.wake()`
- `notify_url_changed()` → `events.on_url_changed(view_id, url)`
- `notify_page_title_changed()` → `events.on_title_changed(view_id, title)`
- Remaining ~25 methods left as default no-ops for v0.1.0.

### Event Translation (`events.rs`)

Stateless mapping from Winit's `KeyEvent`/`MouseButton`/`CursorMoved` into Servo's `InputEvent` variants. Handles key code translation, modifier mapping, and coordinate transformation (window-global → tile-local using the tile's `Rect`).

---

## 4. Compositor

### Architecture

Compositor-centric design. A dedicated compositor module owns the final presentation:

- Servo tiles render to offscreen FBOs (`OffscreenRenderingContext`, children of the window's `WindowRenderingContext`).
- Slint renders to a CPU buffer via `SoftwareRenderer`.
- The compositor blits tile textures at BSP-resolved positions, then alpha-blends the Slint overlay on top.

No GL state is shared between Servo and Slint. The compositor is the only code that touches the window's GL context for drawing.

### Compositor Struct

```rust
pub struct Compositor {
    program: GLuint,          // textured-quad shader (vertex + fragment)
    quad_vao: GLuint,         // unit quad geometry
    slint_texture: GLuint,    // re-uploaded each frame from Slint CPU buffer
}
```

### Frame Draw Sequence

1. `glViewport(0, 0, width, height)` — full window
2. `glClear(COLOR_BUFFER_BIT)` — dark background for empty/gap regions
3. For each `(rect, texture)` from resolved BSP tiles:
   - Set uniform `viewport_rect` (normalized to 0..1)
   - Bind tile's FBO texture
   - Draw quad (shader maps unit quad to target rect)
4. `glEnable(GL_BLEND)`, `glBlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA)`
5. Upload Slint CPU buffer → `slint_texture` via `glTexSubImage2D`
6. Bind `slint_texture`, set uniform `viewport_rect` to full window, draw quad
7. `glDisable(GL_BLEND)`

### Shader

~20 lines of GLSL. Vertex shader transforms unit quad vertices by a `viewport_rect` uniform. Fragment shader samples a `sampler2D`. One program handles both tile blits and the Slint overlay.

---

## 5. BSP Layout Engine

### Data Structures

```rust
pub enum SplitDirection {
    Horizontal,   // top/bottom split
    Vertical,     // left/right split
}

pub enum Node {
    Leaf { view_id: ViewId },
    Branch {
        direction: SplitDirection,
        ratio: f32,            // 0.0..1.0, portion for first child
        first: Box<Node>,
        second: Box<Node>,
    },
}

pub struct BspLayout {
    root: Option<Node>,
    viewport: Rect,
    focused: Option<ViewId>,
}
```

### Operations

- `split(view_id, direction) -> ViewId` — replaces leaf with Branch; children are original leaf + new leaf. Default ratio 0.5.
- `close(view_id)` — removes leaf, promotes sibling to replace parent Branch. If last leaf, root becomes `None`.
- `resize_split(view_id, delta)` — adjusts `ratio` of the parent Branch (clamped to 0.1..0.9).
- `focus_neighbor(view_id, direction) -> Option<ViewId>` — walks tree to find geometrically adjacent leaf.
- `resolve() -> Vec<(ViewId, Rect)>` — recursively resolves tree into flat list of `(view_id, pixel_rect)` pairs.
- `serialize() -> LayoutState` / `deserialize(LayoutState) -> Self` — for session persistence.

### LayoutManager Trait

```rust
pub trait LayoutManager {
    fn split(&mut self, view_id: ViewId, dir: SplitDirection) -> ViewId;
    fn close(&mut self, view_id: ViewId);
    fn resize_split(&mut self, view_id: ViewId, delta: f32);
    fn focus_neighbor(&self, view_id: ViewId, dir: Direction) -> Option<ViewId>;
    fn resolve(&self) -> Vec<(ViewId, Rect)>;
    fn focused(&self) -> Option<ViewId>;
    fn set_focused(&mut self, view_id: ViewId);
    fn serialize(&self) -> LayoutState;
    fn deserialize(state: LayoutState) -> Self;
}
```

Flexible BSP: user chooses split direction at each split. A `Tabbed` implementation can share this trait later (a `Vec<ViewId>` with an active index, all sharing the full viewport).

### Keybindings (Normal Mode)

| Key | Action |
|-----|--------|
| `Ctrl+V` | Split vertical |
| `Ctrl+S` | Split horizontal |
| `h` / `j` / `k` / `l` | Focus neighbor left / down / up / right |
| `H` / `J` / `K` / `L` | Resize split in that direction |
| `q` | Close focused tile |

---

## 6. Input State Machine

### Modes

```
             ┌──────────┐
    ┌───────│  Normal   │────────┐
    │  'i'   └────┬─────┘  ':'   │
    ▼              │ 'f'          ▼
┌──────────┐       │       ┌──────────┐
│  Insert  │       ▼       │ Command  │
└────┬─────┘ ┌──────────┐  └────┬─────┘
     │       │   Hint   │       │
 Esc │       └────┬─────┘   Enter/Esc
     │            │ Esc         │
     └────────────┴─────────────┘
                  ▼
               Normal
```

### Types

```rust
pub enum Mode {
    Normal,
    Insert,
    Command { buffer: String },
    Hint { filter: String, labels: Vec<HintLabel> },
}

pub enum Action {
    // Navigation
    FocusNeighbor(Direction),
    SplitView(SplitDirection),
    CloseView,
    ResizeSplit(Direction, f32),

    // Browsing
    ForwardToServo(CoreInputEvent),  // core-owned type, facade translates to Servo's InputEvent
    Navigate(String),
    Back,
    Forward,
    Reload,

    // Hint
    EnterHintMode,
    HintCharTyped(char),
    ActivateHint(String),

    // Mode
    EnterInsert,
    EnterCommand,
    ExitToNormal,

    // Session
    SaveSession,
    RestoreSession,
}
```

### Behavior

`InputRouter` holds the current `Mode`. On each key event, `handle(key) -> Vec<Action>` returns zero or more actions. The app loop dispatches them.

`InputRouter` is pure — no side effects, no references to Servo or GL. Takes input, returns actions. Trivially testable.

---

## 7. Hint Mode

### Flow

1. User presses `f` in Normal mode → `InputRouter` enters Hint mode.
2. App calls `engine.evaluate_js(focused_tile, HINT_QUERY_SCRIPT, callback)`.
3. Script runs in Servo's DOM, returns JSON array of clickable elements with bounding rects.
4. `hint.rs` generates 2-letter labels from home-row characters.
5. `hud.set_hints(labels_with_positions)` → Slint overlay displays labels.
6. User types first letter → filter narrows, non-matching labels dimmed.
7. User types second letter → exact match → `engine.send_input(view_id, click_at(x, y))`.
8. Return to Normal mode, clear hints.

### DOM Query Script

```javascript
(function() {
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
            y: rect.y + rect.height / 2,
        });
    }
    return JSON.stringify(results);
})()
```

### Label Generation

```rust
const HINT_CHARS: &[u8] = b"asdfghjkl";  // home row, 9 chars

// 2-letter combos: 9 × 9 = 81 hints max
// 3-letter if count > 81: 9 × 9 × 9 = 729
pub fn generate_labels(count: usize) -> Vec<String>;
```

Prefix-based matching: typing first letter immediately narrows to 9 candidates.

---

## 8. Session Persistence

### SQLite Schema

```sql
CREATE TABLE sessions (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL DEFAULT 'default',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tiles (
    id          INTEGER PRIMARY KEY,
    session_id  INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    view_id     INTEGER NOT NULL,
    url         TEXT NOT NULL,
    title       TEXT NOT NULL DEFAULT '',
    scroll_x    REAL NOT NULL DEFAULT 0.0,
    scroll_y    REAL NOT NULL DEFAULT 0.0
);

CREATE TABLE layout_tree (
    id          INTEGER PRIMARY KEY,
    session_id  INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    node_index  INTEGER NOT NULL,
    is_leaf     INTEGER NOT NULL,
    direction   TEXT,
    ratio       REAL,
    view_id     INTEGER,
    UNIQUE(session_id, node_index)
);
```

### Session Manager

```rust
pub struct SessionManager {
    db: rusqlite::Connection,
}

impl SessionManager {
    fn open(path: &Path) -> Self;
    fn save(&self, name: &str, layout: &LayoutState, tiles: &[TileState]);
    fn restore(&self, name: &str) -> Option<(LayoutState, Vec<TileState>)>;
    fn list(&self) -> Vec<SessionInfo>;
    fn delete(&self, name: &str);
    fn autosave(&self, layout: &LayoutState, tiles: &[TileState]);
}
```

### Behavior

- **Autosave:** On every structural change (split, close, navigate), the app calls `autosave()`. Writes to the `"default"` session via `INSERT OR REPLACE`. Crash recovery restores `"default"`.
- **Named sessions:** `:save mywork` and `:restore mywork` via command mode.
- **Tree serialization:** BSP tree flattened to BFS order. Each node gets an index. Branches store direction + ratio. Leaves store `view_id` joining to `tiles` table.
- **DB location:** `~/.local/share/hodei/sessions.db` (XDG on Linux), `~/Library/Application Support/hodei/sessions.db` (macOS).

---

## 9. Slint HUD

### Components (defined in `ui/hud.slint`)

- **Command bar** — bottom of window. Hidden in Normal/Insert mode. Shows input buffer in Command mode.
- **Status line** — bottom-left: current URL and page title. Bottom-right: current mode indicator (NORMAL / INSERT / COMMAND / HINT).
- **Hint labels** — positioned absolutely over tile content. Each label is a small opaque box with 2-letter text. Matching labels highlighted, non-matching dimmed during filtering.
- **Tab indicators** — top of window (when Tabbed layout is used in future). For v0.1.0, shows tile count and focused tile index.

### HUD Bridge (`hud.rs`)

```rust
pub struct Hud {
    slint_instance: HudWindow,
    renderer: slint::SoftwareRenderer,
    buffer: Vec<u8>,
    size: PhysicalSize,
}

impl Hud {
    fn render(&mut self) -> &[u8];
    fn set_command_text(&self, text: &str);
    fn set_hints(&self, hints: Vec<HintOverlay>);
    fn set_status(&self, url: &str, title: &str, mode: &str);
    fn set_tab_bar(&self, tabs: Vec<TabInfo>, active: usize);
    fn clear_hints(&self);
    fn resize(&mut self, size: PhysicalSize);
}
```

All Slint rendering goes through `SoftwareRenderer` → CPU buffer → uploaded as GL texture by the Compositor. No GL context needed for Slint.

---

## 10. Dependencies

### Workspace `Cargo.toml` (key dependencies)

| Crate | Purpose |
|-------|---------|
| `servo` (path = "servo/") | Servo engine via libservo |
| `winit` | Window creation, event loop |
| `slint` | HUD overlay (software renderer feature) |
| `glow` | GL function loading for compositor |
| `raw-window-handle` | Bridge between Winit and Servo's RenderingContext |
| `rusqlite` (bundled feature) | SQLite for session persistence |
| `url` | URL parsing |
| `euclid` | Geometry types (shared with Servo) |
| `serde` + `serde_json` | Hint mode JSON parsing |
| `log` + `env_logger` | Logging |
| `dirs` | XDG/platform-appropriate data directories |
