# Orthogonal × Mairu — Agentic Browser, Phase 1

**Status:** Draft (approved 2026-04-19)
**Scope:** Slices 1, 3, 4 of the orthogonal-agentic roadmap.
**Out of scope (deferred to later specs):** page-driving execution (slice 2), minion-in-a-tile (slice 5), userscripts/CSS, multi-window, embedding mairu binary, push-style continuous capture.

---

## 1. Goals & non-goals

### Goals
- Make orthogonal a first-class mairu host with no Chrome-extension detour.
- An assistant agent the user invokes in a tile that has read access to mairu's full context (memory, nodes, skills) and can pull page state from any orthogonal tile on demand.
- Per-workspace project tagging that flows through every mairu call, with per-tile overrides.
- A small set of mairu-leverage commands: `:scrape`, `:diff`, `:skill`.
- DevTools-lite power-user surface: `:inspect`, `:console`, `:network`.

### Non-goals
- Page-driving execution (`click`, `fill`, `scroll` initiated by the agent).
- Long-running autonomous workflows ("minion in a tile").
- Userscripts / user CSS.
- Multi-window support.
- Embedding the `mairu` binary inside orthogonal.
- Push-style continuous page capture (we use pull-via-tool-calls).

---

## 2. Architecture

Two new pieces in orthogonal, zero forks of mairu:

```
┌──────────────────────────────────────────────┐
│  orthogonal-app (winit loop)                 │
│  ┌────────────────────────────────────────┐  │
│  │ orthogonal-core                        │  │
│  │  ├ workspace.project: Option<String>   │  │
│  │  ├ view.project_override: Option<…>    │  │
│  │  ├ input → :agent / :project / :scrape │  │
│  │  │         :diff / :skill / :inspect   │  │
│  │  │         :console / :network         │  │
│  │  └ devtools.{inspector, console, net}  │  │
│  └────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────┐  │
│  │ orthogonal-mairu (NEW crate)           │  │
│  │  ├ client.rs   — reqwest → :8788       │  │
│  │  ├ server.rs   — axum on 127.0.0.1:?   │  │
│  │  └ auth.rs     — shared-secret token   │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
        │  HTTP                  ▲ HTTP (tool calls)
        ▼                        │
┌──────────────────────────────────────────────┐
│  mairu context-server :8788 (existing)       │
│  + new agent web route (mairu-side work)     │
└──────────────────────────────────────────────┘
```

### Key invariants
- **Orthogonal never speaks LLMs directly.** All intelligence lives in mairu. Orthogonal is a browser that exposes its tile state as tools and renders mairu-served pages.
- **HTTP-only transport.** Both directions: orthogonal → mairu (`reqwest`) and mairu → orthogonal (mairu-side `http.Client` against orthogonal's local Axum server). No stdio, no Unix sockets, no MCP, no native messaging.
- **Mairu lifecycle is independent.** Orthogonal does not spawn or supervise mairu. If mairu's daemon is down, orthogonal degrades gracefully and tells the user how to start it.

---

## 3. Components

### 3.1 `orthogonal-mairu::client`

Thin async wrapper around the mairu HTTP endpoints orthogonal needs. Each method maps to one CLI verb and returns typed Rust:

```rust
pub struct MairuClient {
    base: Url,
    http: reqwest::Client,
}

impl MairuClient {
    pub async fn health(&self) -> Result<()>;
    pub async fn memory_search(&self, q: &str, project: &str, k: usize) -> Result<Vec<Memory>>;
    pub async fn node_search(&self, q: &str, project: &str, k: usize) -> Result<Vec<Node>>;
    pub async fn skill_list(&self, project: &str) -> Result<Vec<Skill>>;
    pub async fn scrape_web(&self, url: &str, project: &str) -> Result<ScrapedPage>;
    pub async fn analyze_diff(&self, repo_path: &Path) -> Result<BlastRadius>;
}
```

Streaming is not orthogonal's concern — agent chat token streaming is owned by the mairu-served chat page rendered inside the agent tile.

### 3.2 `orthogonal-mairu::server`

Axum HTTP server bound to `127.0.0.1:<ephemeral>`. Endpoints:

| Method | Path | Returns |
|---|---|---|
| `GET` | `/tiles` | `[{id, url, title, project, focused: bool}]` |
| `GET` | `/tiles/focused` | `{id, url, title, project, dom, selection, scroll}` |
| `GET` | `/tiles/{id}` | same shape, by id |
| `GET` | `/health` | `{version, workspace, project}` |

All requests require `Authorization: Bearer <token>` matching the contents of `~/.mairu/orthogonal-token` (created on first run, mode 0600). Server handle and selected port are written to `~/.mairu/orthogonal.json` so mairu and the chat page in the agent tile can discover them.

DOM extraction reuses Servo's existing JS-eval path (the same one `search.rs` and `hint.rs` use today): execute `document.documentElement.outerHTML` against the focused webview, return as a string. Selection is `window.getSelection().toString()`.

### 3.3 Project tagging

- `Workspace { project: Option<String> }` — added to existing struct, persisted in session DB via additive migration `004_workspace_project.sql` (the next available number after the existing `001_init.sql`, `002_history_bookmarks.sql`, `003_workspaces.sql`).
- `View { project_override: Option<String> }` — in-memory only, ephemeral by default (does not survive session save in Phase 1).
- Resolution helper: `pub fn effective_project(view: &View, workspace: &Workspace) -> Option<&str>` — tile override wins; else workspace; else `None`.
- HUD adds a `proj:` indicator in the existing status row (slot exists in `hud.slint`).

### 3.4 Commands (added to `input.rs`)

| Command | Behavior |
|---|---|
| `:agent` | Opens (or focuses, if already present in this workspace) an "agent" tile pointing at `http://localhost:8788/agent?project=<p>&orth=<endpoint>&token=<t>`. The tile is just a regular `View`; the agent UX is mairu-served HTML/JS. |
| `:project <name>` | If a tile is focused: sets `view.project_override`. Else: sets `workspace.project`. `:project --workspace foo` always sets the workspace. `:project --clear` clears the override. |
| `:scrape <url>` | Calls `client.scrape_web(url, project)`. Receives content. Opens new tile pointing at the mairu-served reader URL `http://localhost:8788/reader/{node_id}`. |
| `:diff` | Resolves the focused tile's URL → local repo path. Resolution: (1) check explicit map in orthogonal config (`[diff.repos] "github.com/foo/bar" = "/path/to/bar"`); (2) fall back to scanning a configurable list of `repo_roots` (default: `["~"]`) for a directory matching the repo name. Calls `client.analyze_diff(path)`. Shows result in HUD as a collapsible summary. If no local repo resolves, HUD prompts the user to add a mapping. |
| `:skill` | Calls `client.skill_list(project)`. Opens a hint-mode-style overlay listing skills; pick one with home-row keys; runs through mairu's skill executor. |
| `:inspect` | Enters Inspect mode; mouse hover highlights via JS injection (same path as hint mode); click captures element, HUD shows tag/attrs/computed-styles. `Esc` exits. |
| `:console` | Opens HUD JS console panel; lines eval via Servo JS API in focused tile; output appended. Up/Down for history. |
| `:network` | Opens a network-tap tile listing requests for the previously-focused tile. Live-updates. Backed by a Servo network observer registered when DevTools mode is enabled. |

### 3.5 DevTools-lite implementation notes

- **Inspector.** An injected JS shim (loaded into focused tile when mode enters) attaches a `mouseover` listener that draws a 2px outline div over the hovered element. On click, fires a `postMessage` orthogonal already listens to (existing pattern from hint mode). Element info lifts up to Rust via `OrthogonalEvent::Inspect`.
- **Console.** Existing Servo `WebView::evaluate_javascript` returns a JSON-serialized result. HUD console panel is a Slint `TextEdit` + scrollback list. History persisted per-tile (in-memory only in Phase 1) so reopening shows recent lines.
- **Network tap.** Servo exposes a `NetworkListener` trait. Register one per tile when DevTools mode is enabled; buffer `(URL, method, status, ms, byte-size)` in a ring buffer (cap 500). The `:network` tile is a Slint widget reading the buffer.

---

## 4. Data flow

### 4.1 Asking the agent about the focused tile

1. User runs `:agent` → tile opens at `http://localhost:8788/agent?project=orthogonal&orth=http://127.0.0.1:51234&token=…`.
2. User types "what is this article about?" in mairu's chat UI.
3. Mairu's agent decides it needs the active page → calls `GET http://127.0.0.1:51234/tiles/focused` with the bearer token.
4. Orthogonal returns `{url, title, dom, selection}`.
5. Mairu agent summarizes. Tokens stream into the chat tile (mairu-served SSE; orthogonal just renders).

### 4.2 Scrape into a reader tile

1. User runs `:scrape https://news.ycombinator.com/item?id=1`.
2. Orthogonal calls `client.scrape_web(url, project)`.
3. Mairu returns `{node_id, reader_url}`.
4. Orthogonal opens a new tile at `reader_url`. Reader rendering is owned by mairu's dashboard.

### 4.3 Diff blast radius

1. User runs `:diff` while focused on `https://github.com/foo/bar/pull/42`.
2. Orthogonal resolves URL → local repo path via the config-driven map / `repo_roots` scan described in §3.4.
3. Orthogonal calls `client.analyze_diff(path)`.
4. Mairu returns `BlastRadius` (impacted nodes, NL summary).
5. HUD shows a collapsible panel with the summary.

---

## 5. Error handling

- **Mairu daemon down at startup.** HUD shows persistent `[mairu: down]` indicator; mairu-dependent commands fail with `mairu unreachable — start with: mairu context-server -p 8788`. Orthogonal does *not* attempt to spawn mairu.
- **Tool-call auth failure.** Orthogonal logs the failure and rotates the token (writes new one to `~/.mairu/orthogonal-token`); user must refresh the agent tile to pick up the new token via the handshake URL.
- **Project unset.** When running a mairu-dependent command without a resolvable project, HUD prompts `set with :project <name>` and aborts.
- **Network observer registration failure.** `:network` reports `DevTools mode unavailable on this Servo build` and falls back to a no-op.
- **Scrape failure.** Mairu returns an error payload; orthogonal surfaces it in HUD; no tile is opened.

---

## 6. Testing strategy

- **Unit (`orthogonal-mairu/client`).** Mock HTTP via `wiremock`; assert correct endpoint shape, project flag, JSON parsing.
- **Unit (`orthogonal-mairu/server`).** Spin server on ephemeral port; hit it with reqwest; assert auth, JSON shape, 401 on bad token, 404 on unknown tile id.
- **Integration.** Test harness spawns a real `mairu context-server` on a random port + an in-process orthogonal stub; run end-to-end "agent calls back to orthogonal" round-trip.
- **Manual checklist (golden paths):**
  - Open agent tile, ask about the focused page.
  - Switch projects (workspace + tile override) and verify `proj:` HUD indicator.
  - `:scrape` into a reader tile.
  - `:diff` in a known repo.
  - `:skill` palette pick-and-run.
  - `:inspect` an element on a complex page.
  - `:console` runs `console.log(2+2)`, sees `4`.
  - `:network` populates on a fetch-heavy SPA.

---

## 7. Mairu-side work (out of orthogonal's repo)

This spec assumes mairu adds, in its own repo:

- `GET /agent?project=…&orth=…&token=…` — chat web route the agent tile loads.
- Internal SSE endpoint(s) the agent route uses for token streaming.
- `GET /reader/{node_id}` — serves stored scrape as a clean reader page.
- `POST /scrape` — returns `{node_id, reader_url}` (already mostly there per `mairu scrape web`).
- Mairu's agent runtime: ability to make outbound HTTP calls to `orth` endpoint with bearer token (read from query param at handshake, kept server-side per session).

These get their own spec in mairu's repo. Orthogonal's spec only depends on the HTTP shape above.

---

## 8. Sequencing

Order of implementation; each is a meaningful checkpoint:

1. `orthogonal-mairu` crate skeleton + client + server + auth + token file.
2. Project tagging (workspace field, tile override, HUD `proj:` indicator, `:project`).
3. Health check at startup + HUD `[mairu]` indicator.
4. `:agent` command (open tile at mairu URL with handshake).
5. `:scrape`, `:diff`, `:skill`.
6. `:inspect`.
7. `:console`.
8. `:network`.

Steps 1–4 are the load-bearing minimum — once they ship, the agent works end-to-end against any mairu-side `/agent` route. Steps 5–8 add polish and developer surface.
