# Hodei

![Hodei logo-mascot](hodei.png)

A keyboard-first, tiling web browser built in Rust.

Hodei splits the window into a dynamic Binary Space Partitioning (BSP) tree of web views, driven entirely by the keyboard. It embeds the [Servo](https://servo.org) web engine and renders tiles with an OpenGL compositor, overlaying a lightweight Slint-powered HUD for mode, URL, commands, and hint labels.

> **Version:** 0.1.0

---

## Features

- **BSP Tiling Layout** — Split, close, resize, and navigate between tiles with hotkeys.
- **Modal Input** — Normal, Insert, Command, and Hint modes inspired by modal editors.
- **Servo Integration** — Each tile is an independent Servo `WebView` with an offscreen rendering context.
- **GL Compositing** — OpenGL textured-quad compositor blends tile contents with an alpha HUD overlay.
- **Session Persistence** — Save and restore layouts via SQLite.
- **Hint Labels** — Home-row hint labels for keyboard-driven link activation.

---

## Project Structure

```
.
├── crates/
│   ├── hodei-core/      # BSP layout, input routing, HUD bridge, compositor, session persistence
│   ├── hodei-app/       # Winit application loop and integration glue
│   └── hodei-servo/     # Servo engine facade (excluded from root workspace)
├── servo/                    # Servo git submodule
├── ladybird/                 # Ladybird git submodule
├── ui/
│   └── hud.slint             # Slint HUD definition
├── migrations/
│   └── 001_init.sql          # Session DB schema
├── Cargo.toml
└── README.md
```

---

## Prerequisites

- **Rust** 1.94.1 or later
- **Git submodules** initialized:
  ```bash
  git submodule update --init --recursive
  ```
- System dependencies required by **Servo** (see [Servo docs](https://github.com/servo/servo)).

---

## Build

### 1. Build the workspace crates

```bash
cargo build --workspace
```

### 2. Build the Servo facade

`hodei-servo` is excluded from the root workspace because Servo's internal `Cargo.toml` uses `workspace.package` inheritance that conflicts when loaded as a path dependency inside another workspace.

```bash
cd crates/hodei-servo
cargo build
```

Or from the project root:

```bash
cargo build --manifest-path crates/hodei-servo/Cargo.toml
```

---

## Test

Run tests for the workspace crates:

```bash
cargo test --workspace
```

Run tests for the Servo facade:

```bash
cargo test --manifest-path crates/hodei-servo/Cargo.toml
```

---

## Run

Launch the browser:

```bash
cargo run -p hodei-app
```

On first launch, Hodei opens a 1280×720 window and navigates the first tile to `https://servo.org`.

---

## Default Key Bindings

| Mode | Key | Action |
|------|-----|--------|
| **Normal** | `i` | Enter Insert mode (forward keys to web page) |
| **Normal** | `:` | Enter Command mode |
| **Normal** | `f` | Enter Hint mode (generate link hints) |
| **Normal** | `h` `j` `k` `l` | Focus neighbor tile (left / down / up / right) |
| **Normal** | `s` | Split tile horizontally |
| **Normal** | `v` | Split tile vertically |
| **Normal** | `d` | Close focused tile |
| **Any** | `Esc` | Return to Normal mode |
| **Command** | `open <url>` | Navigate focused tile |
| **Command** | `save [name]` | Save current session |
| **Command** | `restore <name>` | Restore a session |
| **Command** | `quit` | Quit the application |

---

## Architecture Overview

1. **Winit Event Loop** (`hodei-app`) drives the application lifecycle.
2. **Input Router** (`hodei-core`) translates keyboard events into `Action`s.
3. **BSP Layout** (`hodei-core`) manages tile geometry and focus.
4. **Servo Facade** (`hodei-servo`) creates one `WebView` + `OffscreenRenderingContext` per tile.
5. **Compositor** (`hodei-core`) reads each tile's FBO into a GL texture and draws quads for each tile, then alpha-blends the HUD on top.
6. **HUD** (`hodei-core`) renders `hud.slint` with a software renderer to an RGBA buffer uploaded as a texture.

---

## Notes

- `hodei-core` uses `rusqlite 0.37` to match Servo's `storage` crate and avoid `libsqlite3-sys` linking conflicts.
- The `servo/` and `ladybird/` directories are shallow-cloned git submodules.
- A benign GL texture warning may appear on first launch; the application continues to run correctly.

---

## License

TBD
