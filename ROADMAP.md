# Orthogonal Roadmap

> A keyboard-first, tiling web browser built in Rust on the Servo engine.

This roadmap uses a **Technology Radar** format. Features are placed in rings that reflect their priority and readiness:

- **Adopt** — Build now. Blockers for daily-driver use.
- **Trial** — Build next. High-value power-user features.
- **Assess** — Prototype / experiment. Complex or speculative.
- **Hold** — Explicitly deferred. Out of current scope or blocked by upstream.

---

## Adopt 🔥
*These are the highest-priority gaps that make Orthogonal feel incomplete today.*

| Feature | Status | Notes |
|---------|--------|-------|
| **Search-in-page** | 🚧 In Progress | UI exists but JS injection is stubbed. Need `window.find()` or DOM highlighter + match count. |
| **Mouse input routing** | 🚧 In Progress | `CoreMouseEvent` types exist but never wired from winit → Servo. Need click-to-focus + scroll. |
| **Zoom controls** | 🚧 In Progress | `+` / `-` / `0` keybindings to zoom the focused tile. |
| **Yank / Clipboard** | 🚧 In Progress | Copy current URL (`yy`) and paste into command bar. |
| **Popup / new-window handling** | 📋 Planned | Servo delegates are no-ops. Need to open popups as new tiles or focused tabs. |
| **Loading progress indicator** | 📋 Planned | HUD should show a spinner or progress bar while page loads. |

---

## Trial 🧪
*Features that make Orthogonal a compelling power-user browser.*

| Feature | Status | Notes |
|---------|--------|-------|
| **Quick marks (global + local)** | 📋 Planned | `m{a-z}` to save a tile position, `'{a-z}` to jump. Global marks across workspaces. |
| **Download manager** | 📋 Planned | Wire Servo download delegate, show progress in HUD, open downloads folder. |
| **Custom user scripts / CSS** | 📋 Planned | Inject per-site or global userscripts and userstyles. |
| **Picture-in-picture** | 📋 Planned | Float video elements above tiles — natural fit for a tiling browser. |
| **Media controls HUD** | 📋 Planned | Play/pause, volume, next/prev bindings that target the active media element. |
| **Error page styling** | 📋 Planned | Custom about:error pages instead of blank screens. |
| **Fullscreen support** | 📋 Planned | `F11` to toggle window fullscreen; per-tile video fullscreen. |

---

## Assess 🔬
*Experimental features that need prototyping or upstream support.*

| Feature | Status | Notes |
|---------|--------|-------|
| **Tabbed layout mode** | 📋 Planned | Alternative to BSP: a single viewport with a tab bar. Toggle with `:layout tabbed`. |
| **Reader mode** | 📋 Planned | Strip clutter, reformat article text. Needs readability-style DOM extraction. |
| **Web notifications** | 📋 Planned | Needs persistent notification bridge + permission store. |
| **Screenshot / capture tile** | 📋 Planned | Save current tile to PNG. Needs access to FBO pixel buffer. |
| **Container tabs / isolated profiles** | 📋 Planned | Separate cookie jars per container (work, personal, etc.). |
| **DevTools integration** | 📋 Planned | Attach Servo devtools or implement a minimal inspector overlay. |
| **Ad / content blocking** | 📋 Planned | Hosts-file or filter-list based blocking. Evaluate `adblock-rust` integration. |

---

## Hold ⏸️
*Features we are explicitly not pursuing in the near term.*

| Feature | Reason |
|---------|--------|
| **Extension system (WebExtensions)** | Massive scope; Servo does not support it. Revisit if engine changes. |
| **Multi-window support** | Would require major refactoring of the single GL context / compositor design. |
| **Password manager / form autofill** | Complex security surface. Use an external password manager for now. |
| **Print support** | Low priority for a keyboard-first tiling browser. |
| **Proxy / VPN settings** | OS-level proxies are sufficient for now. |
| **DNS-over-HTTPS configuration** | Can be handled at OS or network level. |

---

## Legend

| Icon | Meaning |
|------|---------|
| ✅ Done | Fully implemented and tested |
| 🚧 In Progress | Currently being worked on |
| 📋 Planned | Scoped and queued for an upcoming release |
| 🔮 Backlog | Idea stage, not yet committed |

---

## How to Update This Roadmap

1. When a feature moves from **Assess → Trial → Adopt**, shift its row and update the status icon.
2. When a feature ships, move it to a "Shipped" section at the bottom or mark it ✅ in place.
3. Keep **Hold** honest — if scope or upstream support changes, features can move out of Hold.
