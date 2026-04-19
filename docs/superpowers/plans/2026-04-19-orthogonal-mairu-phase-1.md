# Orthogonal × Mairu Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make orthogonal a first-class mairu host: per-workspace project tagging, an agent tile loading mairu's chat URL, mairu-leverage commands (`:scrape`/`:diff`/`:skill`), DevTools-lite (`:inspect`/`:console`/`:network`), all glued together by a small bidirectional HTTP bridge between orthogonal and mairu's `:8788` daemon.

**Architecture:** New `orthogonal-mairu` crate hosts (a) a typed reqwest client to mairu's `:8788`, (b) a tiny Axum HTTP server bound to `127.0.0.1:<ephemeral>` exposing tile-state tools the mairu agent calls back into. A single Tokio runtime on a background thread owns both. The winit/App thread communicates with it via mpsc channels and an `Arc<RwLock<TilesSnapshot>>` published every frame. Project tagging lives on `Workspace` (persisted) with optional per-tile override (in-memory). All eight new commands are added through the existing `parse_command` + `Action` + App-handler pattern.

**Tech Stack:** Rust 2021, Tokio (multi-thread runtime, bg thread), reqwest 0.12 (rustls), axum 0.7, tower 0.5, hyper 1, wiremock 0.6 (test), serde_json, rusqlite (existing), Slint (existing HUD), Servo's `WebView::evaluate_javascript` (existing).

---

## File structure

| File | Status | Responsibility |
|---|---|---|
| `Cargo.toml` (workspace) | Modify | Add `crates/orthogonal-mairu` to `members`. Pin shared async deps in `[workspace.dependencies]`. |
| `crates/orthogonal-mairu/Cargo.toml` | Create | New crate manifest. |
| `crates/orthogonal-mairu/src/lib.rs` | Create | Public exports + `Bridge` façade that owns the runtime thread. |
| `crates/orthogonal-mairu/src/runtime.rs` | Create | Tokio runtime spawned on its own OS thread + shutdown handle. |
| `crates/orthogonal-mairu/src/auth.rs` | Create | Read/write `~/.mairu/orthogonal-token` (mode 0600) and `~/.mairu/orthogonal.json`. |
| `crates/orthogonal-mairu/src/client.rs` | Create | `MairuClient` — typed reqwest wrapper around mairu `:8788`. |
| `crates/orthogonal-mairu/src/server.rs` | Create | Axum server: `/health`, `/tiles`, `/tiles/focused`, `/tiles/{id}`. Bearer-token middleware. |
| `crates/orthogonal-mairu/src/state.rs` | Create | `TilesSnapshot`, `TileSnapshot`, `DomRequest`, `DomResponse` (shared between threads). |
| `crates/orthogonal-mairu/src/error.rs` | Create | `BridgeError` + `Result` alias. |
| `migrations/004_workspace_project.sql` | Create | `ALTER TABLE sessions ADD COLUMN project TEXT;` |
| `crates/orthogonal-core/src/db.rs` | Modify | Include + run migration 004 with column-existence guard. |
| `crates/orthogonal-core/src/types.rs` | Modify | Add `Memory`, `Node`, `Skill`, `ScrapedPage`, `BlastRadius`, `InspectInfo`, `NetEntry`, `TileMeta`. |
| `crates/orthogonal-core/src/view.rs` | Modify | Add `View.project_override: Option<String>`. |
| `crates/orthogonal-core/src/workspace.rs` | Modify | Add `Workspace.project: Option<String>`; wire through save/restore. Add `effective_project` helper. |
| `crates/orthogonal-core/src/session.rs` | Modify | Persist/restore the new `project` column. |
| `crates/orthogonal-core/src/input.rs` | Modify | Parse `:agent`, `:project`, `:scrape`, `:diff`, `:skill`, `:inspect`, `:console`, `:network`. Add matching `Action` variants. |
| `crates/orthogonal-core/src/hud.rs` | Modify | Add `set_project_text`, `set_mairu_status`, `set_console_*`, `set_diff_panel_*`, `set_inspect_info`. |
| `crates/orthogonal-core/src/devtools.rs` | Create | `InspectorShim` (JS), `ConsoleHistory`, `NetworkBuffer` (ring buffer cap 500). |
| `ui/hud.slint` | Modify | Add `proj-text`, `mairu-status`, `console-visible`/`console-lines`, `diff-visible`/`diff-text`, `inspect-visible`/`inspect-text` properties + Slint widgets. |
| `crates/orthogonal-app/Cargo.toml` | Modify | Depend on `orthogonal-mairu`. |
| `crates/orthogonal-app/src/app.rs` | Modify | Spin up `Bridge` on startup. Publish `TilesSnapshot` each frame. Service `DomRequest`s. Implement handlers for all new `Action` variants. |
| `crates/orthogonal-core/src/lib.rs` | Modify | `pub mod devtools;` |
| `crates/orthogonal-app/src/devtools_repo.rs` | Create | URL → local repo path resolver (config map + `repo_roots` scan). |
| `crates/orthogonal-core/src/config.rs` | Modify | Add `[mairu]` section (`base_url`, `default_project`, `repo_roots`, `[diff.repos]` map). |

---

## Task 1: Add `orthogonal-mairu` crate skeleton

**Goal:** New crate compiles, is wired into the workspace, and exposes a no-op `Bridge::start()`/`shutdown()` API the App can call. No behavior yet.

**Files:**
- Create: `crates/orthogonal-mairu/Cargo.toml`
- Create: `crates/orthogonal-mairu/src/lib.rs`
- Create: `crates/orthogonal-mairu/src/error.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add the crate to the workspace**

Edit `Cargo.toml` (workspace root) — change `members` line:

```toml
[workspace]
members = ["crates/orthogonal-core", "crates/orthogonal-app", "crates/orthogonal-mairu"]
resolver = "2"
exclude = ["crates/orthogonal-servo", "servo", "ladybird"]

[workspace.dependencies]
log = "0.4"
env_logger = "0.11"
tokio = { version = "1", features = ["rt-multi-thread", "sync", "macros", "net", "time"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
axum = "0.7"
tower = "0.5"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
```

- [ ] **Step 2: Create the crate manifest**

Create `crates/orthogonal-mairu/Cargo.toml`:

```toml
[package]
name = "orthogonal-mairu"
version = "0.1.0"
edition = "2021"

[dependencies]
orthogonal-core = { path = "../orthogonal-core" }
tokio = { workspace = true }
reqwest = { workspace = true }
axum = { workspace = true }
tower = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
log = { workspace = true }
dirs = "6"
url = "2"
rand = "0.8"

[dev-dependencies]
wiremock = "0.6"
tokio = { version = "1", features = ["rt-multi-thread", "sync", "macros", "net", "time", "test-util"] }
tempfile = "3"
```

- [ ] **Step 3: Create the lib root with a stub `Bridge`**

Create `crates/orthogonal-mairu/src/lib.rs`:

```rust
pub mod auth;
pub mod client;
pub mod error;
pub mod runtime;
pub mod server;
pub mod state;

pub use client::MairuClient;
pub use error::{BridgeError, Result};
pub use state::{DomRequest, DomResponse, TileSnapshot, TilesSnapshot};

use std::sync::Arc;
use std::sync::RwLock;

/// Façade owned by the App. Holds the runtime thread and shared state.
pub struct Bridge {
    pub tiles: Arc<RwLock<TilesSnapshot>>,
    pub dom_request_tx: tokio::sync::mpsc::Sender<DomRequest>,
    pub dom_request_rx: std::sync::Mutex<Option<tokio::sync::mpsc::Receiver<DomRequest>>>,
    pub server_port: u16,
    pub auth_token: String,
    runtime: runtime::RuntimeHandle,
}

impl Bridge {
    /// Stub constructor — fleshed out in later tasks.
    pub fn start(_mairu_base_url: url::Url) -> Result<Self> {
        unimplemented!("filled in by Task 5")
    }
}
```

- [ ] **Step 4: Create the error module and stub the other modules**

Create `crates/orthogonal-mairu/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),
    #[error("mairu unreachable at {url}: {source}")]
    MairuUnreachable { url: String, #[source] source: reqwest::Error },
    #[error("auth: {0}")]
    Auth(String),
    #[error("server: {0}")]
    Server(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BridgeError>;
```

Create empty stubs for the modules referenced by `lib.rs` so the crate compiles:

`crates/orthogonal-mairu/src/auth.rs`, `client.rs`, `runtime.rs`, `server.rs`, `state.rs` — each containing only:

```rust
// Implemented in a later task.
```

- [ ] **Step 5: Verify the crate compiles**

Run: `cargo check -p orthogonal-mairu`
Expected: warnings about unused stub modules are OK; **no errors**, exit 0.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/orthogonal-mairu
git commit -m "feat(mairu): add orthogonal-mairu crate skeleton

Wires the new crate into the workspace with shared async deps.
Bridge façade is a stub — filled out in subsequent tasks."
```

---

## Task 2: `state` module — shared snapshot + DOM request types

**Goal:** Define the data types that flow between the App thread and the bridge thread. No I/O yet.

**Files:**
- Modify: `crates/orthogonal-mairu/src/state.rs`
- Test: inline `#[cfg(test)] mod tests` in the same file

- [ ] **Step 1: Write the failing test**

Replace the contents of `crates/orthogonal-mairu/src/state.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct TilesSnapshot {
    pub tiles: Vec<TileSnapshot>,
    pub focused_id: Option<u64>,
    pub workspace: String,
    pub workspace_project: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TileSnapshot {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub project: Option<String>,
}

#[derive(Debug)]
pub struct DomRequest {
    pub view_id: u64,
    pub reply_tx: tokio::sync::oneshot::Sender<DomResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DomResponse {
    pub url: String,
    pub title: String,
    pub dom: String,
    pub selection: String,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_snapshot_default_is_empty() {
        let s = TilesSnapshot::default();
        assert!(s.tiles.is_empty());
        assert!(s.focused_id.is_none());
        assert_eq!(s.workspace, "");
    }

    #[test]
    fn tile_snapshot_serializes_to_expected_json() {
        let t = TileSnapshot {
            id: 42,
            url: "https://example.com".into(),
            title: "Ex".into(),
            project: Some("orthogonal".into()),
        };
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v["id"], 42);
        assert_eq!(v["url"], "https://example.com");
        assert_eq!(v["project"], "orthogonal");
    }

    #[test]
    fn tiles_snapshot_serializes_with_focused_id() {
        let s = TilesSnapshot {
            tiles: vec![],
            focused_id: Some(7),
            workspace: "default".into(),
            workspace_project: None,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["focused_id"], 7);
        assert_eq!(v["workspace"], "default");
        assert!(v["workspace_project"].is_null());
    }
}
```

- [ ] **Step 2: Run the tests, verify they pass**

Run: `cargo test -p orthogonal-mairu --lib state`
Expected: 3 passed, 0 failed.

- [ ] **Step 3: Commit**

```bash
git add crates/orthogonal-mairu/src/state.rs
git commit -m "feat(mairu): add shared state types for the bridge

TilesSnapshot is the App-published, server-readable view of tile
state. DomRequest / DomResponse carry async DOM-fetch round-trips
between the axum server and the App thread."
```

---

## Task 3: `auth` module — token file management

**Goal:** Read or create `~/.mairu/orthogonal-token` (32-byte hex, mode 0600 on Unix). Write `~/.mairu/orthogonal.json` with port + token + version for mairu/dashboard discovery.

**Files:**
- Modify: `crates/orthogonal-mairu/src/auth.rs`

- [ ] **Step 1: Write the failing test**

Replace the contents of `crates/orthogonal-mairu/src/auth.rs`:

```rust
use crate::error::{BridgeError, Result};
use rand::RngCore;
use serde::Serialize;
use std::path::{Path, PathBuf};

const TOKEN_BYTES: usize = 32;

pub fn mairu_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        BridgeError::Auth("could not resolve $HOME".into())
    })?;
    let dir = home.join(".mairu");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn token_path() -> Result<PathBuf> {
    Ok(mairu_dir()?.join("orthogonal-token"))
}

pub fn descriptor_path() -> Result<PathBuf> {
    Ok(mairu_dir()?.join("orthogonal.json"))
}

/// Read the token file; create one with a fresh random token if missing.
pub fn load_or_create_token() -> Result<String> {
    let path = token_path()?;
    if path.exists() {
        let s = std::fs::read_to_string(&path)?;
        let trimmed = s.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    let token = generate_token();
    write_token_file(&path, &token)?;
    Ok(token)
}

/// Force-rotate the token (used after auth failures).
pub fn rotate_token() -> Result<String> {
    let path = token_path()?;
    let token = generate_token();
    write_token_file(&path, &token)?;
    Ok(token)
}

#[derive(Debug, Serialize)]
pub struct Descriptor {
    pub version: &'static str,
    pub port: u16,
    pub host: &'static str,
    pub token: String,
}

pub fn write_descriptor(port: u16, token: &str) -> Result<()> {
    let path = descriptor_path()?;
    let d = Descriptor {
        version: env!("CARGO_PKG_VERSION"),
        port,
        host: "127.0.0.1",
        token: token.to_string(),
    };
    let s = serde_json::to_string_pretty(&d)?;
    std::fs::write(&path, s)?;
    set_mode_0600(&path)?;
    Ok(())
}

fn generate_token() -> String {
    let mut buf = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

fn write_token_file(path: &Path, token: &str) -> Result<()> {
    std::fs::write(path, token)?;
    set_mode_0600(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_mode_0600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perm = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perm)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode_0600(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Override $HOME for the test so we don't touch the user's real ~/.mairu.
    fn with_temp_home<R>(f: impl FnOnce() -> R) -> R {
        let tmp = TempDir::new().unwrap();
        let prev = std::env::var_os("HOME");
        std::env::set_var("HOME", tmp.path());
        let r = f();
        if let Some(p) = prev {
            std::env::set_var("HOME", p);
        } else {
            std::env::remove_var("HOME");
        }
        drop(tmp);
        r
    }

    #[test]
    fn token_is_created_if_missing() {
        with_temp_home(|| {
            let t = load_or_create_token().unwrap();
            assert_eq!(t.len(), TOKEN_BYTES * 2); // hex
            // second call returns same token
            let t2 = load_or_create_token().unwrap();
            assert_eq!(t, t2);
        });
    }

    #[test]
    fn rotate_returns_new_token() {
        with_temp_home(|| {
            let t1 = load_or_create_token().unwrap();
            let t2 = rotate_token().unwrap();
            assert_ne!(t1, t2);
            // load now returns the rotated one
            let t3 = load_or_create_token().unwrap();
            assert_eq!(t2, t3);
        });
    }

    #[test]
    fn descriptor_roundtrip() {
        with_temp_home(|| {
            let token = load_or_create_token().unwrap();
            write_descriptor(54321, &token).unwrap();
            let s = std::fs::read_to_string(descriptor_path().unwrap()).unwrap();
            let v: serde_json::Value = serde_json::from_str(&s).unwrap();
            assert_eq!(v["port"], 54321);
            assert_eq!(v["host"], "127.0.0.1");
            assert_eq!(v["token"], token);
        });
    }

    #[cfg(unix)]
    #[test]
    fn token_file_is_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        with_temp_home(|| {
            load_or_create_token().unwrap();
            let meta = std::fs::metadata(token_path().unwrap()).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        });
    }
}
```

- [ ] **Step 2: Run the tests, verify they pass**

Run: `cargo test -p orthogonal-mairu --lib auth -- --test-threads=1`
(Single-threaded because the tests mutate `$HOME`.)
Expected: 4 passed (5 on Unix).

- [ ] **Step 3: Commit**

```bash
git add crates/orthogonal-mairu/src/auth.rs
git commit -m "feat(mairu): token + descriptor file management

~/.mairu/orthogonal-token holds a 32-byte hex token (mode 0600).
~/.mairu/orthogonal.json publishes host/port/token so mairu and
its dashboard can discover the orthogonal tool server."
```

---

## Task 4: `MairuClient` — typed HTTP wrapper around `:8788`

**Goal:** Async client returning typed Rust values for the mairu endpoints orthogonal needs. Tested with `wiremock`.

**Files:**
- Modify: `crates/orthogonal-mairu/src/client.rs`

- [ ] **Step 1: Write the failing tests + minimal types**

Replace the contents of `crates/orthogonal-mairu/src/client.rs`:

```rust
use crate::error::{BridgeError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone)]
pub struct MairuClient {
    base: Url,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Memory {
    pub id: String,
    pub content: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub importance: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Node {
    pub uri: String,
    pub name: String,
    #[serde(default)]
    pub abstract_text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Skill {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ScrapedPage {
    pub node_id: String,
    pub reader_url: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct BlastRadius {
    pub summary: String,
    #[serde(default)]
    pub impacted: Vec<String>,
}

impl MairuClient {
    pub fn new(base: Url) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("reqwest client builds");
        Self { base, http }
    }

    pub fn base_url(&self) -> &Url {
        &self.base
    }

    pub async fn health(&self) -> Result<()> {
        let url = self.base.join("health").map_err(BridgeError::from)?;
        let resp = self.http.get(url.clone()).send().await.map_err(|e| {
            BridgeError::MairuUnreachable { url: url.to_string(), source: e }
        })?;
        if !resp.status().is_success() {
            return Err(BridgeError::Server(format!("health: {}", resp.status())));
        }
        Ok(())
    }

    pub async fn memory_search(&self, q: &str, project: &str, k: usize) -> Result<Vec<Memory>> {
        let url = self.base.join("memory/search")?;
        let resp = self.http.get(url)
            .query(&[("q", q), ("project", project), ("k", &k.to_string())])
            .send().await?
            .error_for_status()?
            .json::<Vec<Memory>>().await?;
        Ok(resp)
    }

    pub async fn node_search(&self, q: &str, project: &str, k: usize) -> Result<Vec<Node>> {
        let url = self.base.join("node/search")?;
        let resp = self.http.get(url)
            .query(&[("q", q), ("project", project), ("k", &k.to_string())])
            .send().await?
            .error_for_status()?
            .json::<Vec<Node>>().await?;
        Ok(resp)
    }

    pub async fn skill_list(&self, project: &str) -> Result<Vec<Skill>> {
        let url = self.base.join("skill/list")?;
        let resp = self.http.get(url)
            .query(&[("project", project)])
            .send().await?
            .error_for_status()?
            .json::<Vec<Skill>>().await?;
        Ok(resp)
    }

    pub async fn scrape_web(&self, url: &str, project: &str) -> Result<ScrapedPage> {
        let endpoint = self.base.join("scrape/web")?;
        let body = serde_json::json!({ "url": url, "project": project });
        let resp = self.http.post(endpoint)
            .json(&body)
            .send().await?
            .error_for_status()?
            .json::<ScrapedPage>().await?;
        Ok(resp)
    }

    pub async fn analyze_diff(&self, repo_path: &Path) -> Result<BlastRadius> {
        let endpoint = self.base.join("analyze/diff")?;
        let body = serde_json::json!({ "path": repo_path.to_string_lossy() });
        let resp = self.http.post(endpoint)
            .json(&body)
            .send().await?
            .error_for_status()?
            .json::<BlastRadius>().await?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn client_for(server: &MockServer) -> MairuClient {
        MairuClient::new(Url::parse(&server.uri()).unwrap().join("/").unwrap())
    }

    #[tokio::test]
    async fn health_succeeds_on_2xx() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/health"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server).await;
        let c = client_for(&server).await;
        c.health().await.unwrap();
    }

    #[tokio::test]
    async fn health_errors_on_500() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/health"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server).await;
        let c = client_for(&server).await;
        let err = c.health().await.unwrap_err();
        assert!(matches!(err, BridgeError::Server(_)));
    }

    #[tokio::test]
    async fn memory_search_passes_project_and_k() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/memory/search"))
            .and(query_param("q", "auth"))
            .and(query_param("project", "orthogonal"))
            .and(query_param("k", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "1", "content": "use jwt", "category": "decision", "importance": 7}
            ])))
            .mount(&server).await;
        let c = client_for(&server).await;
        let mems = c.memory_search("auth", "orthogonal", 5).await.unwrap();
        assert_eq!(mems.len(), 1);
        assert_eq!(mems[0].content, "use jwt");
    }

    #[tokio::test]
    async fn scrape_web_posts_json_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/scrape/web"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "node_id": "abc", "reader_url": "http://localhost:8788/reader/abc", "title": "Hi"
            })))
            .mount(&server).await;
        let c = client_for(&server).await;
        let page = c.scrape_web("https://x.com", "orthogonal").await.unwrap();
        assert_eq!(page.node_id, "abc");
        assert_eq!(page.reader_url, "http://localhost:8788/reader/abc");
    }

    #[tokio::test]
    async fn unreachable_returns_typed_error() {
        // Use a port nothing listens on
        let c = MairuClient::new(Url::parse("http://127.0.0.1:1").unwrap());
        let err = c.health().await.unwrap_err();
        assert!(matches!(err, BridgeError::MairuUnreachable { .. }));
    }
}
```

- [ ] **Step 2: Run the tests, verify they pass**

Run: `cargo test -p orthogonal-mairu --lib client`
Expected: 4 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/orthogonal-mairu/src/client.rs
git commit -m "feat(mairu): typed reqwest client for mairu :8788

Wraps the seven endpoints orthogonal needs (health, memory/search,
node/search, skill/list, scrape/web, analyze/diff). All calls take
explicit project. Tested via wiremock."
```

---

## Task 5: Tokio runtime + Axum tool server + `Bridge` constructor

**Goal:** A real `Bridge::start()` that spawns a Tokio runtime on its own thread, binds an Axum server on `127.0.0.1:0`, returns the chosen port, and writes the descriptor file. The server has `/health`, `/tiles`, `/tiles/focused`, `/tiles/{id}` with bearer-token middleware. DOM-needing endpoints proxy via `DomRequest` channel.

**Files:**
- Modify: `crates/orthogonal-mairu/src/runtime.rs`
- Modify: `crates/orthogonal-mairu/src/server.rs`
- Modify: `crates/orthogonal-mairu/src/lib.rs`

- [ ] **Step 1: Implement the runtime handle**

Replace the contents of `crates/orthogonal-mairu/src/runtime.rs`:

```rust
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::runtime::Builder;
use tokio::sync::oneshot;

/// Owns the dedicated OS thread that runs the Tokio runtime.
pub struct RuntimeHandle {
    pub handle: tokio::runtime::Handle,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl RuntimeHandle {
    pub fn spawn(name: &'static str) -> std::io::Result<Self> {
        let (handle_tx, handle_rx) = std::sync::mpsc::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let join = std::thread::Builder::new().name(name.into()).spawn(move || {
            let rt = Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name(name)
                .build()
                .expect("tokio runtime builds");
            handle_tx.send(rt.handle().clone()).expect("send rt handle");
            rt.block_on(async move {
                let _ = shutdown_rx.await;
            });
        })?;

        let handle = handle_rx.recv().expect("rt handle arrives");
        Ok(Self { handle, shutdown_tx: Some(shutdown_tx), join: Some(join) })
    }

    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for RuntimeHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// Force `Send + Sync` ergonomics for the App (Arc-wrapped).
unsafe impl Send for RuntimeHandle {}
unsafe impl Sync for RuntimeHandle {}

#[allow(dead_code)]
fn _arc_check() -> Arc<RuntimeHandle> {
    panic!("compile-time check only");
}
```

- [ ] **Step 2: Implement the server**

Replace the contents of `crates/orthogonal-mairu/src/server.rs`:

```rust
use crate::error::{BridgeError, Result};
use crate::state::{DomRequest, DomResponse, TilesSnapshot};
use axum::extract::{Path as AxPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

#[derive(Clone)]
pub struct ServerState {
    pub tiles: Arc<RwLock<TilesSnapshot>>,
    pub dom_request_tx: mpsc::Sender<DomRequest>,
    pub auth_token: Arc<RwLock<String>>,
    pub workspace: Arc<RwLock<String>>,
}

pub async fn bind_and_serve(
    state: ServerState,
) -> Result<(u16, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await
        .map_err(BridgeError::from)?;
    let port = listener.local_addr().map_err(BridgeError::from)?.port();

    let app = Router::new()
        .route("/health", get(health))
        .route("/tiles", get(list_tiles))
        .route("/tiles/focused", get(focused_tile))
        .route("/tiles/:id", get(tile_by_id))
        .with_state(state);

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("orthogonal tool server: {e}");
        }
    });
    Ok((port, handle))
}

fn check_auth(headers: &HeaderMap, token: &str) -> std::result::Result<(), StatusCode> {
    let h = headers.get("authorization").and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let stripped = h.strip_prefix("Bearer ").ok_or(StatusCode::UNAUTHORIZED)?;
    if stripped != token { return Err(StatusCode::UNAUTHORIZED); }
    Ok(())
}

async fn health(State(s): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.auth_token.read().unwrap()) { return c.into_response(); }
    let workspace = s.workspace.read().unwrap().clone();
    let tiles = s.tiles.read().unwrap();
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "workspace": workspace,
        "workspace_project": tiles.workspace_project,
    })).into_response()
}

async fn list_tiles(State(s): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.auth_token.read().unwrap()) { return c.into_response(); }
    let snap = s.tiles.read().unwrap().clone();
    Json(snap).into_response()
}

async fn focused_tile(State(s): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.auth_token.read().unwrap()) { return c.into_response(); }
    let focused_id = match s.tiles.read().unwrap().focused_id {
        Some(id) => id,
        None => return (StatusCode::NOT_FOUND, "no focused tile").into_response(),
    };
    fetch_dom(&s, focused_id).await
}

async fn tile_by_id(
    AxPath(id): AxPath<u64>,
    State(s): State<ServerState>,
    headers: HeaderMap,
) -> axum::response::Response {
    if let Err(c) = check_auth(&headers, &s.auth_token.read().unwrap()) { return c.into_response(); }
    let exists = s.tiles.read().unwrap().tiles.iter().any(|t| t.id == id);
    if !exists {
        return (StatusCode::NOT_FOUND, "no such tile").into_response();
    }
    fetch_dom(&s, id).await
}

async fn fetch_dom(s: &ServerState, view_id: u64) -> axum::response::Response {
    let (reply_tx, reply_rx) = oneshot::channel();
    let req = DomRequest { view_id, reply_tx };
    if s.dom_request_tx.send(req).await.is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, "app shutting down").into_response();
    }
    match tokio::time::timeout(std::time::Duration::from_secs(10), reply_rx).await {
        Ok(Ok(resp)) => Json(resp).into_response(),
        Ok(Err(_)) => (StatusCode::INTERNAL_SERVER_ERROR, "dom reply dropped").into_response(),
        Err(_) => (StatusCode::GATEWAY_TIMEOUT, "dom fetch timed out").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DomResponse, TileSnapshot};

    fn make_state() -> (ServerState, mpsc::Receiver<DomRequest>) {
        let (tx, rx) = mpsc::channel(8);
        let mut snap = TilesSnapshot::default();
        snap.workspace = "default".into();
        snap.tiles = vec![TileSnapshot {
            id: 1, url: "https://example.com".into(), title: "Ex".into(), project: None,
        }];
        snap.focused_id = Some(1);
        let s = ServerState {
            tiles: Arc::new(RwLock::new(snap)),
            dom_request_tx: tx,
            auth_token: Arc::new(RwLock::new("secret".into())),
            workspace: Arc::new(RwLock::new("default".into())),
        };
        (s, rx)
    }

    #[tokio::test]
    async fn missing_auth_returns_401() {
        let (state, _rx) = make_state();
        let (port, _h) = bind_and_serve(state).await.unwrap();
        let resp = reqwest::get(format!("http://127.0.0.1:{port}/health")).await.unwrap();
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test]
    async fn list_tiles_returns_snapshot() {
        let (state, _rx) = make_state();
        let (port, _h) = bind_and_serve(state).await.unwrap();
        let resp = reqwest::Client::new().get(format!("http://127.0.0.1:{port}/tiles"))
            .header("Authorization", "Bearer secret").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let v: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(v["focused_id"], 1);
        assert_eq!(v["tiles"][0]["url"], "https://example.com");
    }

    #[tokio::test]
    async fn focused_returns_404_when_no_focus() {
        let (mut state, _rx) = make_state();
        // Replace tiles with no focused_id
        {
            let mut t = state.tiles.write().unwrap();
            t.focused_id = None;
        }
        let (port, _h) = bind_and_serve(state).await.unwrap();
        let resp = reqwest::Client::new().get(format!("http://127.0.0.1:{port}/tiles/focused"))
            .header("Authorization", "Bearer secret").send().await.unwrap();
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn focused_proxies_dom_request_and_returns_response() {
        let (state, mut rx) = make_state();
        let (port, _h) = bind_and_serve(state).await.unwrap();

        // Spawn the "App" side: listen for DomRequests and reply.
        let h = tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                let _ = req.reply_tx.send(DomResponse {
                    url: "https://example.com".into(),
                    title: "Ex".into(),
                    dom: "<html></html>".into(),
                    selection: "".into(),
                    scroll_x: 0.0,
                    scroll_y: 0.0,
                });
            }
        });

        let resp = reqwest::Client::new().get(format!("http://127.0.0.1:{port}/tiles/focused"))
            .header("Authorization", "Bearer secret").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let v: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(v["dom"], "<html></html>");
        h.await.unwrap();
    }
}
```

- [ ] **Step 3: Implement `Bridge::start`**

Replace the contents of `crates/orthogonal-mairu/src/lib.rs`:

```rust
pub mod auth;
pub mod client;
pub mod error;
pub mod runtime;
pub mod server;
pub mod state;

pub use client::MairuClient;
pub use error::{BridgeError, Result};
pub use state::{DomRequest, DomResponse, TileSnapshot, TilesSnapshot};

use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

pub struct Bridge {
    pub tiles: Arc<RwLock<TilesSnapshot>>,
    pub dom_request_rx: std::sync::Mutex<Option<mpsc::Receiver<DomRequest>>>,
    pub server_port: u16,
    pub auth_token: Arc<RwLock<String>>,
    pub workspace: Arc<RwLock<String>>,
    pub client: MairuClient,
    runtime: runtime::RuntimeHandle,
    _server_task: tokio::task::JoinHandle<()>,
}

impl Bridge {
    /// Spawn the runtime, bind the tool server, write the descriptor file.
    pub fn start(mairu_base_url: url::Url) -> Result<Self> {
        let runtime = runtime::RuntimeHandle::spawn("orthogonal-mairu")
            .map_err(BridgeError::from)?;

        let token = auth::load_or_create_token()?;
        let auth_token = Arc::new(RwLock::new(token.clone()));
        let workspace = Arc::new(RwLock::new(String::new()));
        let tiles = Arc::new(RwLock::new(TilesSnapshot::default()));

        let (dom_tx, dom_rx) = mpsc::channel(32);

        let state = server::ServerState {
            tiles: tiles.clone(),
            dom_request_tx: dom_tx,
            auth_token: auth_token.clone(),
            workspace: workspace.clone(),
        };
        let handle = runtime.handle.clone();
        let (port, server_task) = handle.block_on(server::bind_and_serve(state))?;

        auth::write_descriptor(port, &token)?;

        let client = MairuClient::new(mairu_base_url);

        Ok(Self {
            tiles,
            dom_request_rx: std::sync::Mutex::new(Some(dom_rx)),
            server_port: port,
            auth_token,
            workspace,
            client,
            runtime,
            _server_task: server_task,
        })
    }

    /// Take the receiver out so the App can drain it on its own thread.
    pub fn take_dom_rx(&self) -> Option<mpsc::Receiver<DomRequest>> {
        self.dom_request_rx.lock().unwrap().take()
    }

    /// Give the App a way to run async client calls synchronously from the winit thread.
    pub fn block_on<F: std::future::Future>(&self, fut: F) -> F::Output {
        self.runtime.handle.block_on(fut)
    }

    pub fn shutdown(mut self) {
        self.runtime.shutdown();
    }
}
```

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cargo test -p orthogonal-mairu`
Expected: all (state + auth + client + server) green.

- [ ] **Step 5: Add an end-to-end Bridge test**

Append to `crates/orthogonal-mairu/src/lib.rs`:

```rust
#[cfg(test)]
mod bridge_tests {
    use super::*;

    fn with_temp_home<R>(f: impl FnOnce() -> R) -> R {
        let tmp = tempfile::TempDir::new().unwrap();
        let prev = std::env::var_os("HOME");
        std::env::set_var("HOME", tmp.path());
        let r = f();
        if let Some(p) = prev { std::env::set_var("HOME", p); } else { std::env::remove_var("HOME"); }
        r
    }

    #[test]
    fn start_brings_up_server_and_writes_descriptor() {
        with_temp_home(|| {
            let bridge = Bridge::start(url::Url::parse("http://127.0.0.1:8788").unwrap()).unwrap();
            assert!(bridge.server_port > 0);
            let descriptor = std::fs::read_to_string(auth::descriptor_path().unwrap()).unwrap();
            assert!(descriptor.contains(&bridge.server_port.to_string()));
            bridge.shutdown();
        });
    }
}
```

Run: `cargo test -p orthogonal-mairu --lib bridge_tests -- --test-threads=1`
Expected: 1 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/orthogonal-mairu
git commit -m "feat(mairu): live Bridge with Tokio runtime + Axum tool server

Bridge::start spawns a dedicated runtime thread, binds 127.0.0.1:0,
serves /health, /tiles, /tiles/focused, /tiles/:id with bearer auth.
DOM-needing endpoints proxy through an mpsc channel the App drains."
```

---

## Task 6: Migration 004 — `project` column on `sessions`

**Goal:** Schema supports per-workspace project. Backwards-compatible (column is nullable).

**Files:**
- Create: `migrations/004_workspace_project.sql`
- Modify: `crates/orthogonal-core/src/db.rs`

- [ ] **Step 1: Write the migration**

Create `migrations/004_workspace_project.sql`:

```sql
ALTER TABLE sessions ADD COLUMN project TEXT;
```

- [ ] **Step 2: Wire it into `db.rs` with a column-existence guard**

Edit `crates/orthogonal-core/src/db.rs`, replace the migration constants and `run_migrations`:

```rust
const MIGRATION_001: &str = include_str!("../../../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../../../migrations/002_history_bookmarks.sql");
const MIGRATION_003: &str = include_str!("../../../migrations/003_workspaces.sql");
const MIGRATION_004: &str = include_str!("../../../migrations/004_workspace_project.sql");

// (open_database / open_database_in_memory unchanged)

fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(MIGRATION_001)?;
    conn.execute_batch(MIGRATION_002)?;
    let has_is_active: bool = conn.prepare("SELECT is_active FROM sessions LIMIT 0").is_ok();
    if !has_is_active { conn.execute_batch(MIGRATION_003)?; }
    let has_project: bool = conn.prepare("SELECT project FROM sessions LIMIT 0").is_ok();
    if !has_project { conn.execute_batch(MIGRATION_004)?; }
    Ok(())
}
```

- [ ] **Step 3: Add a test asserting the column exists**

Append to the `tests` module in `crates/orthogonal-core/src/db.rs`:

```rust
    #[test]
    fn migration_004_adds_project_column() {
        let conn = open_database_in_memory().unwrap();
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(sessions)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(cols.contains(&"project".to_string()), "got cols: {:?}", cols);
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p orthogonal-core --lib db`
Expected: all green, new test passes.

- [ ] **Step 5: Commit**

```bash
git add migrations/004_workspace_project.sql crates/orthogonal-core/src/db.rs
git commit -m "feat(db): migration 004 adds nullable project column to sessions"
```

---

## Task 7: `Workspace.project` field + persistence

**Goal:** WorkspaceManager carries an optional project per workspace, persisted in the new column.

**Files:**
- Modify: `crates/orthogonal-core/src/workspace.rs`
- Modify: `crates/orthogonal-core/src/session.rs`

- [ ] **Step 1: Extend `SessionManager::save` and `restore` with project**

Edit `crates/orthogonal-core/src/session.rs`. Change `save` to accept an extra `project: Option<&str>` parameter:

```rust
    pub fn save(
        &self,
        name: &str,
        nodes: &[LayoutNodeRow],
        tiles: &[TileRow],
        focused: Option<ViewId>,
        project: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM sessions WHERE name = ?1", params![name])?;
        tx.execute(
            "INSERT INTO sessions (name, project) VALUES (?1, ?2)",
            params![name, project],
        )?;
        let session_id = tx.last_insert_rowid();
        // (tile + layout inserts unchanged)
        for tile in tiles { /* same as before */
            tx.execute(
                "INSERT INTO tiles (session_id, view_id, url, title, scroll_x, scroll_y) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![session_id, tile.view_id.0 as i64, tile.url, tile.title, tile.scroll_x, tile.scroll_y],
            )?;
        }
        for node in nodes {
            let dir_str = node.direction.map(|d| match d {
                SplitDirection::Horizontal => "h",
                SplitDirection::Vertical => "v",
            });
            tx.execute(
                "INSERT INTO layout_tree (session_id, node_index, is_leaf, direction, ratio, view_id, focused_view_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![session_id, node.node_index as i64, node.is_leaf as i32, dir_str, node.ratio,
                        node.view_id.map(|v| v.0 as i64), focused.map(|v| v.0 as i64)],
            )?;
        }
        tx.commit()
    }
```

Change `restore` to return project too:

```rust
    pub fn restore(&self, name: &str)
        -> Result<Option<(Vec<LayoutNodeRow>, Vec<TileRow>, Option<ViewId>, Option<String>)>, rusqlite::Error>
    {
        let row: Option<(i64, Option<String>)> = self.conn.query_row(
            "SELECT id, project FROM sessions WHERE name = ?1",
            params![name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).ok();
        let (session_id, project) = match row {
            Some(r) => r,
            None => return Ok(None),
        };
        // (existing tile + layout reads unchanged)
        let mut stmt = self.conn.prepare(
            "SELECT view_id, url, title, scroll_x, scroll_y FROM tiles WHERE session_id = ?1"
        )?;
        let tiles: Vec<TileRow> = stmt.query_map(params![session_id], |row| {
            Ok(TileRow {
                view_id: ViewId(row.get::<_, i64>(0)? as u64),
                url: row.get(1)?, title: row.get(2)?,
                scroll_x: row.get(3)?, scroll_y: row.get(4)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        let mut stmt = self.conn.prepare(
            "SELECT node_index, is_leaf, direction, ratio, view_id, focused_view_id FROM layout_tree WHERE session_id = ?1 ORDER BY node_index"
        )?;
        let mut focused: Option<ViewId> = None;
        let nodes: Vec<LayoutNodeRow> = stmt.query_map(params![session_id], |row| {
            let fv: Option<i64> = row.get(5)?;
            if let Some(fv) = fv { focused = Some(ViewId(fv as u64)); }
            let dir_str: Option<String> = row.get(2)?;
            Ok(LayoutNodeRow {
                node_index: row.get::<_, i64>(0)? as u32,
                is_leaf: row.get::<_, i32>(1)? != 0,
                direction: dir_str.map(|s| if s == "h" { SplitDirection::Horizontal } else { SplitDirection::Vertical }),
                ratio: row.get(3)?,
                view_id: row.get::<_, Option<i64>>(4)?.map(|v| ViewId(v as u64)),
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(Some((nodes, tiles, focused, project)))
    }
```

Also update `autosave` to forward `None` (or thread through):

```rust
    pub fn autosave(&self, nodes: &[LayoutNodeRow], tiles: &[TileRow],
                    focused: Option<ViewId>, project: Option<&str>)
        -> Result<(), rusqlite::Error>
    {
        self.save("default", nodes, tiles, focused, project)
    }
```

Update existing tests in `session.rs` to pass `None` to `save`/`autosave` and to destructure the 4-tuple from `restore`.

- [ ] **Step 2: Add a test for the new project field**

Append to the `tests` module of `session.rs`:

```rust
    #[test]
    fn project_roundtrips_through_save_and_restore() {
        let sm = make_session_manager();
        let nodes = sample_nodes();
        let tiles = sample_tiles();
        sm.save("work", &nodes, &tiles, Some(ViewId(1)), Some("orthogonal")).unwrap();
        let (_, _, _, proj) = sm.restore("work").unwrap().unwrap();
        assert_eq!(proj.as_deref(), Some("orthogonal"));
    }
```

- [ ] **Step 3: Update WorkspaceManager**

Edit `crates/orthogonal-core/src/workspace.rs`. Add `project: Option<String>` to `WorkspaceState`, plumb through `save_active` / `switch_to`:

```rust
pub struct WorkspaceState {
    pub nodes: Vec<LayoutNodeRow>,
    pub tiles: Vec<TileRow>,
    pub focused: Option<ViewId>,
    pub project: Option<String>,
}

impl WorkspaceManager {
    pub fn save_active(
        &mut self,
        nodes: &[LayoutNodeRow],
        tiles: &[TileRow],
        focused: Option<ViewId>,
        project: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        if self.active.is_empty() { return Ok(()); }
        self.cache.insert(self.active.clone(), WorkspaceState {
            nodes: nodes.to_vec(), tiles: tiles.to_vec(), focused,
            project: project.map(|s| s.to_string()),
        });
        self.session.save(&self.active, nodes, tiles, focused, project)
    }

    pub fn switch_to(
        &mut self,
        name: &str,
        current_nodes: &[LayoutNodeRow],
        current_tiles: &[TileRow],
        current_focused: Option<ViewId>,
        current_project: Option<&str>,
    ) -> Result<Option<WorkspaceState>, rusqlite::Error> {
        if !self.active.is_empty() {
            self.cache.insert(self.active.clone(), WorkspaceState {
                nodes: current_nodes.to_vec(), tiles: current_tiles.to_vec(),
                focused: current_focused, project: current_project.map(|s| s.to_string()),
            });
            self.session.save(&self.active, current_nodes, current_tiles, current_focused, current_project)?;
        }
        self.active = name.to_string();
        if let Some(state) = self.cache.get(name) {
            return Ok(Some(WorkspaceState {
                nodes: state.nodes.clone(), tiles: state.tiles.clone(),
                focused: state.focused, project: state.project.clone(),
            }));
        }
        match self.session.restore(name)? {
            Some((nodes, tiles, focused, project)) => {
                let state = WorkspaceState {
                    nodes: nodes.clone(), tiles: tiles.clone(), focused, project: project.clone(),
                };
                self.cache.insert(name.to_string(), WorkspaceState { nodes, tiles, focused, project });
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    pub fn create_new(&mut self, name: &str) {
        self.active = name.to_string();
        self.cache.insert(name.to_string(), WorkspaceState {
            nodes: vec![], tiles: vec![], focused: None, project: None,
        });
    }

    pub fn set_active_project(&mut self, project: Option<String>) {
        if self.active.is_empty() { return; }
        if let Some(state) = self.cache.get_mut(&self.active) {
            state.project = project;
        }
    }

    pub fn active_project(&self) -> Option<&str> {
        if self.active.is_empty() { return None; }
        self.cache.get(&self.active).and_then(|s| s.project.as_deref())
    }
}
```

Update existing `workspace.rs` tests to thread `None` for project where required, and add:

```rust
    #[test]
    fn project_can_be_set_and_read() {
        let mut wm = make_workspace_manager();
        wm.set_active("work");
        wm.save_active(&[], &[], None, Some("mairu")).unwrap();
        wm.set_active_project(Some("orthogonal".into()));
        assert_eq!(wm.active_project(), Some("orthogonal"));
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p orthogonal-core`
Expected: all green. Fix any callers that broke (usage sites in `app.rs` are updated in Task 11).

- [ ] **Step 5: Commit**

```bash
git add crates/orthogonal-core/src/workspace.rs crates/orthogonal-core/src/session.rs
git commit -m "feat(workspace): persist optional project per workspace"
```

---

## Task 8: `View.project_override` + `effective_project` helper

**Goal:** Per-tile project override, in-memory only. Resolution helper that prefers override → workspace → none.

**Files:**
- Modify: `crates/orthogonal-core/src/view.rs`

- [ ] **Step 1: Write the failing tests**

Edit `crates/orthogonal-core/src/view.rs`. Change the `View` struct and add `effective_project`:

```rust
pub struct View {
    pub id: ViewId,
    pub url: String,
    pub title: String,
    pub dirty: bool,
    pub project_override: Option<String>,
}
```

Update `ViewManager::create` and `create_with_id` to initialize `project_override: None`.

Add (at module top-level):

```rust
pub fn effective_project<'a>(view: Option<&'a View>, workspace_project: Option<&'a str>) -> Option<&'a str> {
    if let Some(v) = view {
        if let Some(p) = v.project_override.as_deref() { return Some(p); }
    }
    workspace_project
}
```

Append to the `tests` module:

```rust
    #[test]
    fn effective_project_prefers_override() {
        let mut vm = ViewManager::new();
        let id = vm.create("https://x.com");
        vm.get_mut(id).unwrap().project_override = Some("override".into());
        let v = vm.get(id);
        assert_eq!(effective_project(v, Some("workspace")), Some("override"));
    }

    #[test]
    fn effective_project_falls_back_to_workspace() {
        let mut vm = ViewManager::new();
        let id = vm.create("https://x.com");
        let v = vm.get(id);
        assert_eq!(effective_project(v, Some("workspace")), Some("workspace"));
    }

    #[test]
    fn effective_project_none_when_neither_set() {
        let mut vm = ViewManager::new();
        let id = vm.create("https://x.com");
        let v = vm.get(id);
        assert_eq!(effective_project(v, None), None);
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p orthogonal-core --lib view`
Expected: all green.

- [ ] **Step 3: Commit**

```bash
git add crates/orthogonal-core/src/view.rs
git commit -m "feat(view): add per-tile project_override + resolution helper"
```

---

## Task 9: HUD additions (`proj`, `mairu`, `console`, `diff`, `inspect`)

**Goal:** Slint HUD exposes new properties and renders new panels. Rust `Hud` struct gains setters for each. No behavior wiring yet — just UI surface.

**Files:**
- Modify: `ui/hud.slint`
- Modify: `crates/orthogonal-core/src/hud.rs`

- [ ] **Step 1: Add new properties to `hud.slint`**

In `ui/hud.slint`, add to the `HudWindow` properties list (after `shortcuts-visible`):

```slint
    in property <string> proj-text: "";
    in property <string> mairu-status: "ok";  // "ok" | "down" | "checking"
    in property <bool> console-visible: false;
    in property <[string]> console-lines: [];
    in property <string> console-input: "";
    in property <bool> diff-visible: false;
    in property <string> diff-text: "";
    in property <bool> inspect-visible: false;
    in property <string> inspect-text: "";
    in property <bool> agent-skill-visible: false;
    in property <[SuggestionItem]> skill-items: [];
```

In the bottom status bar, add a `proj-text` element after `tile-count` (between the `[N]` count and the mode badge):

```slint
            Text {
                text: root.proj-text != "" ? "[proj:" + root.proj-text + "]" : "";
                color: #80c0ff;
                font-size: 12px;
                horizontal-alignment: right;
                vertical-alignment: center;
                min-width: 0px;
            }
            Text {
                text: root.mairu-status == "ok" ? "[mairu]" :
                      root.mairu-status == "down" ? "[mairu:down]" : "[mairu?]";
                color: root.mairu-status == "ok" ? #00ff88 :
                       root.mairu-status == "down" ? #ff4444 : #c0c0c0;
                font-size: 12px;
                horizontal-alignment: right;
                vertical-alignment: center;
                min-width: 0px;
            }
```

Add a console panel after the search bar (above status bar when visible):

```slint
    Rectangle {
        visible: root.console-visible;
        y: parent.height - 240px;
        width: parent.width;
        height: 192px;
        background: #0d0d1aee;

        VerticalLayout {
            padding: 8px;
            spacing: 2px;
            for line in root.console-lines: Text {
                text: line;
                color: #c0c0c0;
                font-size: 11px;
                overflow: elide;
            }
            Text {
                text: "> " + root.console-input;
                color: #00ff88;
                font-size: 11px;
            }
        }
    }
```

Add a diff panel and an inspect line near the bottom (above the status bar):

```slint
    Rectangle {
        visible: root.diff-visible;
        y: parent.height - 24px - self.height;
        width: parent.width;
        height: 160px;
        background: #0d0d1aee;

        Text {
            x: 8px;
            y: 8px;
            text: root.diff-text;
            color: #e0e0e0;
            font-size: 11px;
            wrap: word-wrap;
        }
    }

    Rectangle {
        visible: root.inspect-visible;
        y: parent.height - 24px - 24px;
        width: parent.width;
        height: 24px;
        background: #1a1a2eee;

        Text {
            x: 8px;
            text: root.inspect-text;
            color: #ffcc00;
            font-size: 11px;
            vertical-alignment: center;
        }
    }
```

- [ ] **Step 2: Add Rust setters in `hud.rs`**

In `crates/orthogonal-core/src/hud.rs`, append to the `impl Hud { ... }` block (just before its closing brace):

```rust
    pub fn set_project_text(&self, text: &str) {
        self.hud_instance.set_proj_text(SharedString::from(text));
    }

    pub fn set_mairu_status(&self, status: &str) {
        self.hud_instance.set_mairu_status(SharedString::from(status));
    }

    pub fn set_console_visible(&self, v: bool) {
        self.hud_instance.set_console_visible(v);
    }

    pub fn set_console_lines(&self, lines: Vec<String>) {
        let model = VecModel::from(lines.into_iter().map(SharedString::from).collect::<Vec<_>>());
        self.hud_instance.set_console_lines(ModelRc::new(model));
    }

    pub fn set_console_input(&self, text: &str) {
        self.hud_instance.set_console_input(SharedString::from(text));
    }

    pub fn set_diff_panel(&self, visible: bool, text: &str) {
        self.hud_instance.set_diff_visible(visible);
        self.hud_instance.set_diff_text(SharedString::from(text));
    }

    pub fn set_inspect_info(&self, visible: bool, text: &str) {
        self.hud_instance.set_inspect_visible(visible);
        self.hud_instance.set_inspect_text(SharedString::from(text));
    }
```

- [ ] **Step 3: Run a build to verify Slint codegen sees the new properties**

Run: `cargo build -p orthogonal-core`
Expected: clean build, no errors.

- [ ] **Step 4: Commit**

```bash
git add ui/hud.slint crates/orthogonal-core/src/hud.rs
git commit -m "feat(hud): add proj/mairu/console/diff/inspect surfaces

Properties + Slint widgets and Rust setters. No behavior yet —
the App wires these in subsequent tasks."
```

---

## Task 10: New `Action` variants + `:` command parsing

**Goal:** Input router parses `:agent`, `:project`, `:scrape`, `:diff`, `:skill`, `:inspect`, `:console`, `:network`. Each yields a typed `Action` variant. No App-side handling yet.

**Files:**
- Modify: `crates/orthogonal-core/src/input.rs`

- [ ] **Step 1: Write failing tests**

Append to the `tests` module of `crates/orthogonal-core/src/input.rs`:

```rust
    fn type_command(router: &mut InputRouter, cmd: &str) -> Vec<Action> {
        router.handle(&key(':'));
        for c in cmd.chars() { router.handle(&key(c)); }
        router.handle(&special(CoreKey::Enter))
    }

    #[test]
    fn command_agent_emits_open_agent_tile() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "agent");
        assert_eq!(actions, vec![Action::OpenAgentTile]);
    }

    #[test]
    fn command_project_workspace_sets_workspace_project() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "project --workspace mairu");
        assert_eq!(actions, vec![Action::SetWorkspaceProject(Some("mairu".into()))]);
    }

    #[test]
    fn command_project_sets_tile_override() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "project foo");
        assert_eq!(actions, vec![Action::SetTileProject(Some("foo".into()))]);
    }

    #[test]
    fn command_project_clear_clears_tile_override() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "project --clear");
        assert_eq!(actions, vec![Action::SetTileProject(None)]);
    }

    #[test]
    fn command_scrape_takes_url() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "scrape https://x.com");
        assert_eq!(actions, vec![Action::Scrape("https://x.com".into())]);
    }

    #[test]
    fn command_diff_no_args() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "diff");
        assert_eq!(actions, vec![Action::DiffBlastRadius]);
    }

    #[test]
    fn command_skill_opens_palette() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "skill");
        assert_eq!(actions, vec![Action::OpenSkillPalette]);
    }

    #[test]
    fn command_inspect_enters_inspect_mode() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "inspect");
        assert_eq!(actions, vec![Action::EnterInspect]);
    }

    #[test]
    fn command_console_toggles_console() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "console");
        assert_eq!(actions, vec![Action::ToggleConsole]);
    }

    #[test]
    fn command_network_opens_network_tile() {
        let mut router = InputRouter::new();
        let actions = type_command(&mut router, "network");
        assert_eq!(actions, vec![Action::OpenNetworkTile]);
    }
```

- [ ] **Step 2: Add the `Action` variants**

Edit the `Action` enum in `crates/orthogonal-core/src/input.rs`, append:

```rust
    OpenAgentTile,
    SetWorkspaceProject(Option<String>),
    SetTileProject(Option<String>),
    Scrape(String),
    DiffBlastRadius,
    OpenSkillPalette,
    EnterInspect,
    ToggleConsole,
    OpenNetworkTile,
```

- [ ] **Step 3: Implement command parsing**

Extend the `match parts.first().copied()` in `parse_command` (in the same file), adding before the `_ => vec![]` fallback:

```rust
            Some("agent") => vec![Action::OpenAgentTile],
            Some("project") => {
                let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");
                if arg == "--clear" {
                    vec![Action::SetTileProject(None)]
                } else if let Some(rest) = arg.strip_prefix("--workspace") {
                    let name = rest.trim();
                    if name.is_empty() {
                        vec![Action::SetWorkspaceProject(None)]
                    } else {
                        vec![Action::SetWorkspaceProject(Some(name.to_string()))]
                    }
                } else if arg.is_empty() {
                    vec![]
                } else {
                    vec![Action::SetTileProject(Some(arg.to_string()))]
                }
            }
            Some("scrape") => parts.get(1).map(|u| vec![Action::Scrape(u.to_string())]).unwrap_or_default(),
            Some("diff") => vec![Action::DiffBlastRadius],
            Some("skill") => vec![Action::OpenSkillPalette],
            Some("inspect") => vec![Action::EnterInspect],
            Some("console") => vec![Action::ToggleConsole],
            Some("network") => vec![Action::OpenNetworkTile],
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test -p orthogonal-core --lib input`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add crates/orthogonal-core/src/input.rs
git commit -m "feat(input): parse :agent/:project/:scrape/:diff/:skill/:inspect/:console/:network"
```

---

## Task 11: App-side wiring of `Bridge`, snapshot publishing, DOM service

**Goal:** App owns a `Bridge`, publishes `TilesSnapshot` whenever tile state changes, drains `DomRequest`s and replies via `evaluate_js`. Project state from `Workspace` is included in snapshots. No new commands handled yet — those land in subsequent tasks.

**Files:**
- Modify: `crates/orthogonal-app/Cargo.toml`
- Modify: `crates/orthogonal-app/src/app.rs`
- Modify: `crates/orthogonal-core/src/config.rs`

- [ ] **Step 1: Add the dep**

Edit `crates/orthogonal-app/Cargo.toml`, add:

```toml
orthogonal-mairu = { path = "../orthogonal-mairu" }
url = "2"
```

(`url` is already there — leave it.)

- [ ] **Step 2: Add a `[mairu]` section to config**

Edit `crates/orthogonal-core/src/config.rs`. Add:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MairuConfig {
    pub base_url: String,
    pub default_project: Option<String>,
    pub repo_roots: Vec<String>,
    #[serde(default)]
    pub diff_repos: HashMap<String, String>,
}

impl Default for MairuConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8788".into(),
            default_project: None,
            repo_roots: vec!["~".into()],
            diff_repos: HashMap::new(),
        }
    }
}
```

Add `pub mairu: MairuConfig,` to the `Config` struct, and `mairu: MairuConfig::default(),` to the `Default for Config` block.

Add a test:

```rust
    #[test]
    fn parses_mairu_section() {
        let toml_str = r#"
[mairu]
base_url = "http://localhost:9999"
default_project = "orthogonal"
repo_roots = ["~/code", "~/work"]

[mairu.diff_repos]
"github.com/foo/bar" = "/abs/path/bar"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mairu.base_url, "http://localhost:9999");
        assert_eq!(config.mairu.default_project.as_deref(), Some("orthogonal"));
        assert_eq!(config.mairu.repo_roots.len(), 2);
        assert_eq!(config.mairu.diff_repos.get("github.com/foo/bar").map(|s| s.as_str()), Some("/abs/path/bar"));
    }
```

Run: `cargo test -p orthogonal-core --lib config`
Expected: all green.

- [ ] **Step 3: Start the bridge in `App::new`**

Edit `crates/orthogonal-app/src/app.rs`. Add to imports:

```rust
use orthogonal_mairu::{Bridge, DomRequest, DomResponse, TileSnapshot, TilesSnapshot};
```

Add fields to `App`:

```rust
    bridge: Option<Bridge>,
    dom_request_rx: Option<tokio::sync::mpsc::Receiver<DomRequest>>,
```

In `App::new`, after the `let input = ...` line, add:

```rust
        let mairu_url = url::Url::parse(&config.mairu.base_url)
            .unwrap_or_else(|_| url::Url::parse("http://127.0.0.1:8788").unwrap());
        let bridge = match Bridge::start(mairu_url) {
            Ok(b) => Some(b),
            Err(e) => { log::error!("mairu bridge failed to start: {e}"); None }
        };
        let dom_request_rx = bridge.as_ref().and_then(|b| b.take_dom_rx());
```

Initialize the new fields in the `Self { ... }` struct literal:

```rust
            bridge,
            dom_request_rx,
```

- [ ] **Step 4: Add the snapshot publisher**

In `app.rs`, add an `App` method (place near other helpers):

```rust
    fn publish_tiles_snapshot(&self) {
        let Some(bridge) = self.bridge.as_ref() else { return };
        let workspace = self.workspace.as_ref().map(|w| w.active_name().to_string()).unwrap_or_default();
        let workspace_project = self.workspace.as_ref().and_then(|w| w.active_project()).map(String::from);
        let focused = self.layout.focused();
        let tiles = self.layout.resolve().into_iter().filter_map(|(id, _rect)| {
            let v = self.views.get(id)?;
            let project = orthogonal_core::view::effective_project(Some(v), workspace_project.as_deref()).map(String::from);
            Some(TileSnapshot {
                id: id.0,
                url: v.url.clone(),
                title: v.title.clone(),
                project,
            })
        }).collect();
        let snap = TilesSnapshot {
            tiles,
            focused_id: focused.map(|v| v.0),
            workspace: workspace.clone(),
            workspace_project,
        };
        *bridge.tiles.write().unwrap() = snap;
        *bridge.workspace.write().unwrap() = workspace;
    }
```

> Note: `BspLayout::resolve()` already returns `Vec<(ViewId, Rect)>` for every leaf and `BspLayout::focused() -> Option<ViewId>` exists. Other helpers used in later tasks: `set_focused(view_id)` (NOT `set_focus`) and `split(target_view_id, SplitDirection, new_view_id)` (NOT `split_focused`). Verified via `grep "pub fn"` on `crates/orthogonal-core/src/layout.rs`.

Call `self.publish_tiles_snapshot();` at the end of any place tile state changes — at minimum:
- after creating/destroying a view
- after navigation (URL change) — i.e. inside the `MetadataEvent::UrlChanged` and `TitleChanged` branches
- after focus changes (`FocusNeighbor`, `FocusNext`, `FocusPrev`)
- after workspace switches

A simple safer approach: call it once per frame at the top of the redraw path (idempotent + cheap).

- [ ] **Step 5: Drain `DomRequest`s every frame**

Add another helper:

```rust
    fn service_dom_requests(&mut self) {
        let Some(rx) = self.dom_request_rx.as_mut() else { return };
        let Some(engine) = self.engine.as_ref() else { return };
        while let Ok(req) = rx.try_recv() {
            let view_id = ViewId(req.view_id);
            let url_title = self.views.get(view_id).map(|v| (v.url.clone(), v.title.clone()));
            let Some((url, title)) = url_title else {
                let _ = req.reply_tx.send(DomResponse {
                    url: String::new(), title: String::new(),
                    dom: String::new(), selection: String::new(),
                    scroll_x: 0.0, scroll_y: 0.0,
                });
                continue;
            };
            let reply_tx = req.reply_tx;
            engine.evaluate_js(view_id, r#"(function(){
                return JSON.stringify({
                    dom: document.documentElement.outerHTML,
                    selection: window.getSelection ? window.getSelection().toString() : "",
                    scroll_x: window.scrollX || 0,
                    scroll_y: window.scrollY || 0
                });
            })()"#, Box::new(move |result| {
                let parsed = result.ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s.trim_matches('"')).ok());
                let resp = if let Some(v) = parsed {
                    DomResponse {
                        url, title,
                        dom: v["dom"].as_str().unwrap_or("").to_string(),
                        selection: v["selection"].as_str().unwrap_or("").to_string(),
                        scroll_x: v["scroll_x"].as_f64().unwrap_or(0.0),
                        scroll_y: v["scroll_y"].as_f64().unwrap_or(0.0),
                    }
                } else {
                    DomResponse {
                        url, title, dom: String::new(), selection: String::new(),
                        scroll_x: 0.0, scroll_y: 0.0,
                    }
                };
                let _ = reply_tx.send(resp);
            }));
        }
    }
```

Call `self.service_dom_requests();` at the top of the main winit redraw / Servo-tick handler (the `UserEvent::ServoTick` branch).

- [ ] **Step 6: Build and smoke-run**

Run: `cargo build --workspace`
Expected: clean build.

Run: `cargo run -p orthogonal-app` and verify it boots without panic. Stop it with `Esc`/quit. Confirm `~/.mairu/orthogonal.json` was written and contains a non-zero port.

- [ ] **Step 7: Commit**

```bash
git add crates/orthogonal-app crates/orthogonal-core/src/config.rs
git commit -m "feat(app): start Bridge on launch, publish tile snapshots, service DOM tool calls"
```

---

## Task 12: Health check + `[mairu]` HUD indicator + `:project` handler

**Goal:** Show mairu daemon status in HUD. Implement the `:project` command end-to-end (workspace + per-tile override). The HUD `proj:` indicator updates from the snapshot.

**Files:**
- Modify: `crates/orthogonal-app/src/app.rs`

- [ ] **Step 1: Periodic health check**

Add a field to `App`:

```rust
    last_mairu_check: std::time::Instant,
    mairu_status: &'static str, // "ok" | "down" | "checking"
```

Initialize in `App::new`: `last_mairu_check: std::time::Instant::now() - std::time::Duration::from_secs(60), mairu_status: "checking",`

Add a helper:

```rust
    fn check_mairu_health(&mut self) {
        let Some(bridge) = self.bridge.as_ref() else { self.mairu_status = "down"; return };
        if self.last_mairu_check.elapsed() < std::time::Duration::from_secs(15) { return; }
        self.last_mairu_check = std::time::Instant::now();
        let client = bridge.client.clone();
        let result = bridge.block_on(async move { client.health().await });
        self.mairu_status = if result.is_ok() { "ok" } else { "down" };
        if let Some(hud) = self.hud.as_ref() {
            hud.set_mairu_status(self.mairu_status);
        }
    }
```

Call `self.check_mairu_health();` once per `UserEvent::ServoTick` (it self-throttles to once every 15s).

- [ ] **Step 2: Push `proj:` text to HUD**

Add to `publish_tiles_snapshot` (just after writing the snapshot):

```rust
        if let Some(hud) = self.hud.as_ref() {
            let focused = self.layout.focused();
            let workspace_project = self.workspace.as_ref().and_then(|w| w.active_project());
            let proj = focused
                .and_then(|id| self.views.get(id))
                .and_then(|v| v.project_override.as_deref())
                .or(workspace_project)
                .unwrap_or("");
            hud.set_project_text(proj);
        }
```

- [ ] **Step 3: Handle `Action::SetWorkspaceProject` and `Action::SetTileProject`**

Add to the App's action-dispatch `match action { ... }`:

```rust
            Action::SetWorkspaceProject(name) => {
                if let Some(ws) = self.workspace.as_mut() {
                    ws.set_active_project(name.clone());
                }
                self.publish_tiles_snapshot();
            }
            Action::SetTileProject(name) => {
                if let Some(focused) = self.layout.focused() {
                    if let Some(v) = self.views.get_mut(focused) {
                        v.project_override = name.clone();
                    }
                }
                self.publish_tiles_snapshot();
            }
```

- [ ] **Step 4: Build + smoke-test**

Run: `cargo build --workspace`
Expected: clean build.

Run: `cargo run -p orthogonal-app`. With mairu daemon stopped, verify `[mairu:down]` appears in HUD. Start mairu: `mairu context-server -p 8788 &` (in another terminal). Within 15s the indicator flips to `[mairu]`. Try `:project foo` → see `[proj:foo]`. Try `:project --workspace bar` → still see `[proj:foo]` (override wins). `:project --clear` → see `[proj:bar]`.

- [ ] **Step 5: Commit**

```bash
git add crates/orthogonal-app/src/app.rs
git commit -m "feat(app): :project command + HUD indicators for project + mairu daemon"
```

---

## Task 13: `:agent` command — open mairu chat tile with handshake

**Goal:** `:agent` opens (or focuses) a tile pointing at `<mairu_base_url>/agent?project=<p>&orth=<endpoint>&token=<t>`. If a tile in the current workspace is already at an `/agent` URL, focus it instead of opening another.

**Files:**
- Modify: `crates/orthogonal-app/src/app.rs`

- [ ] **Step 1: Build the URL helper**

Add to `App`:

```rust
    fn build_agent_url(&self) -> Option<String> {
        let bridge = self.bridge.as_ref()?;
        let project = self.workspace.as_ref()
            .and_then(|w| w.active_project())
            .map(String::from)
            .or_else(|| self.config.mairu.default_project.clone())
            .unwrap_or_default();
        let token = bridge.auth_token.read().unwrap().clone();
        let orth = format!("http://127.0.0.1:{}", bridge.server_port);
        let mut url = url::Url::parse(&self.config.mairu.base_url).ok()?;
        url.set_path("/agent");
        url.query_pairs_mut()
            .append_pair("project", &project)
            .append_pair("orth", &orth)
            .append_pair("token", &token);
        Some(url.to_string())
    }
```

- [ ] **Step 2: Find-or-open helper**

Add to `App`:

```rust
    fn focus_or_open_agent_tile(&mut self) {
        let Some(target_url) = self.build_agent_url() else {
            log::warn!(":agent — bridge not ready");
            return;
        };
        let target_prefix = {
            let mut u = url::Url::parse(&target_url).unwrap();
            u.set_query(None);
            u.to_string()
        };
        let existing = self.layout.resolve().into_iter().find_map(|(id, _)| {
            self.views.get(id)
                .filter(|v| v.url.starts_with(&target_prefix))
                .map(|_| id)
        });
        if let Some(id) = existing {
            self.layout.set_focused(id);
            // Reload with fresh token-bearing URL in case token rotated
            if let Some(engine) = self.engine.as_ref() {
                engine.navigate(id, &target_url);
            }
            self.publish_tiles_snapshot();
            return;
        }
        // Open as a new tile by splitting the focused tile vertically.
        if let Some(focused) = self.layout.focused() {
            if let Some(engine) = self.engine.as_mut() {
                let new_id = self.views.create(&target_url);
                self.layout.split(focused, SplitDirection::Vertical, new_id);
                engine.create_tile(new_id, &target_url);
                self.layout.set_focused(new_id);
            }
        }
        self.publish_tiles_snapshot();
    }
```

> Note: the exact "split + create + focus" sequence above must mirror how `Action::SplitView` is handled elsewhere in `app.rs` — read that handler and copy its order. If it differs, follow the existing pattern verbatim and only abstract the URL-building.

- [ ] **Step 3: Wire `Action::OpenAgentTile`**

Add to the action-dispatch match:

```rust
            Action::OpenAgentTile => self.focus_or_open_agent_tile(),
```

- [ ] **Step 4: Smoke-test**

Run: `cargo run -p orthogonal-app`. Type `:agent`. With mairu running, a new tile should open at `http://127.0.0.1:8788/agent?...`. (The `/agent` route is mairu-side work and may 404 today — that's OK; orthogonal's behavior is the request itself.) Verify the URL HUD shows the agent URL with project/orth/token query params.

- [ ] **Step 5: Commit**

```bash
git add crates/orthogonal-app/src/app.rs
git commit -m "feat(app): :agent opens or focuses mairu chat tile with handshake"
```

---

## Task 14: `:scrape <url>` — open scraped reader tile

**Goal:** `:scrape https://x.com` calls `mairu.scrape_web(url, project)`, then opens the returned `reader_url` in a new tile. If project is unset, HUD prompts.

**Files:**
- Modify: `crates/orthogonal-app/src/app.rs`

- [ ] **Step 1: Project resolution helper**

Add to `App`:

```rust
    fn current_project(&self) -> Option<String> {
        let workspace_project = self.workspace.as_ref().and_then(|w| w.active_project()).map(String::from);
        let focused = self.layout.focused();
        let view = focused.and_then(|id| self.views.get(id));
        orthogonal_core::view::effective_project(view, workspace_project.as_deref())
            .map(String::from)
            .or_else(|| self.config.mairu.default_project.clone())
    }
```

- [ ] **Step 2: Handle `Action::Scrape`**

```rust
            Action::Scrape(url) => {
                let Some(bridge) = self.bridge.as_ref() else {
                    self.set_status("mairu bridge unavailable");
                    return;
                };
                let Some(project) = self.current_project() else {
                    self.set_status("set a project first: :project <name>");
                    return;
                };
                let client = bridge.client.clone();
                let url_clone = url.clone();
                let project_clone = project.clone();
                let result = bridge.block_on(async move {
                    client.scrape_web(&url_clone, &project_clone).await
                });
                match result {
                    Ok(page) => {
                        if let Some(focused) = self.layout.focused() {
                            if let Some(engine) = self.engine.as_mut() {
                                let new_id = self.views.create(&page.reader_url);
                                self.layout.split(focused, SplitDirection::Vertical, new_id);
                                engine.create_tile(new_id, &page.reader_url);
                                self.layout.set_focused(new_id);
                            }
                        }
                        self.publish_tiles_snapshot();
                    }
                    Err(e) => {
                        log::error!(":scrape failed: {e}");
                        self.set_status(&format!("scrape failed: {e}"));
                    }
                }
            }
```

> Note: `set_status` is referenced throughout subsequent tasks. The HUD already has a `status_text` Slint property and `Hud::set_status_text(&str)` Rust setter (verified via grep on `crates/orthogonal-app/src/app.rs:786`). Define this helper on `App` once, here:
>
> ```rust
> fn set_status(&self, msg: &str) {
>     if let Some(hud) = self.hud.as_ref() { hud.set_status_text(msg); }
> }
> ```

- [ ] **Step 3: Smoke-test**

Run mairu, then `cargo run -p orthogonal-app`. Set a project (`:project orthogonal`), then `:scrape https://news.ycombinator.com`. A new tile should open at the reader URL.

- [ ] **Step 4: Commit**

```bash
git add crates/orthogonal-app/src/app.rs
git commit -m "feat(app): :scrape <url> opens scraped reader tile via mairu"
```

---

## Task 15: `:diff` — repo resolution + blast-radius HUD panel

**Goal:** `:diff` resolves the focused tile's URL → local repo path, calls `mairu.analyze_diff`, shows summary in the HUD diff panel.

**Files:**
- Create: `crates/orthogonal-app/src/devtools_repo.rs`
- Modify: `crates/orthogonal-app/src/app.rs`

- [ ] **Step 1: Repo resolver with tests**

Create `crates/orthogonal-app/src/devtools_repo.rs`:

```rust
use std::collections::HashMap;
use std::path::PathBuf;

/// Try to resolve a tile URL to a local repo path.
/// Strategy: explicit map → scan repo_roots for a directory matching the repo name.
pub fn resolve_repo(
    url: &str,
    explicit_map: &HashMap<String, String>,
    repo_roots: &[String],
) -> Option<PathBuf> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let path = parsed.path();
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();

    // Look up in explicit map: try "host/seg0/seg1", then "host/seg0".
    if segments.len() >= 2 {
        let key = format!("{host}/{}/{}", segments[0], segments[1]);
        if let Some(p) = explicit_map.get(&key) { return Some(expand(p)); }
    }
    if !segments.is_empty() {
        let key = format!("{host}/{}", segments[0]);
        if let Some(p) = explicit_map.get(&key) { return Some(expand(p)); }
    }

    // Scan repo_roots for a directory named like the last segment.
    let candidate_name = segments.iter().rev().find(|s| !s.is_empty())?;
    for root in repo_roots {
        let root = expand(root);
        let candidate = root.join(candidate_name);
        if candidate.is_dir() && candidate.join(".git").exists() {
            return Some(candidate);
        }
    }
    None
}

fn expand(s: &str) -> PathBuf {
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() { return home.join(stripped); }
    }
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn explicit_map_two_segments_wins() {
        let mut map = HashMap::new();
        map.insert("github.com/foo/bar".to_string(), "/abs/bar".to_string());
        let r = resolve_repo("https://github.com/foo/bar/pull/1", &map, &[]).unwrap();
        assert_eq!(r, PathBuf::from("/abs/bar"));
    }

    #[test]
    fn missing_in_map_returns_none_when_no_roots() {
        let r = resolve_repo("https://github.com/foo/bar", &HashMap::new(), &[]);
        assert!(r.is_none());
    }

    #[test]
    fn scans_repo_roots_for_matching_git_dir() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("bar");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let r = resolve_repo(
            "https://github.com/foo/bar",
            &HashMap::new(),
            &[tmp.path().to_string_lossy().to_string()],
        ).unwrap();
        assert_eq!(r, repo);
    }
}
```

Wire it as a module: edit `crates/orthogonal-app/src/main.rs` (or wherever modules are declared in the app crate — read the file to confirm) and add `mod devtools_repo;`. If `app.rs` declares its own modules, add it there.

- [ ] **Step 2: Run repo-resolver tests**

Run: `cargo test -p orthogonal-app --lib devtools_repo`
Expected: 3 passed.

- [ ] **Step 3: Handle `Action::DiffBlastRadius`**

In `app.rs`:

```rust
            Action::DiffBlastRadius => {
                let Some(bridge) = self.bridge.as_ref() else { self.set_status("mairu bridge unavailable"); return };
                let Some(focused) = self.layout.focused() else { return };
                let Some(view) = self.views.get(focused) else { return };
                let path = match crate::devtools_repo::resolve_repo(
                    &view.url, &self.config.mairu.diff_repos, &self.config.mairu.repo_roots,
                ) {
                    Some(p) => p,
                    None => {
                        self.set_status("no local repo for this URL — add to [mairu.diff_repos]");
                        return;
                    }
                };
                let client = bridge.client.clone();
                let result = bridge.block_on(async move { client.analyze_diff(&path).await });
                match result {
                    Ok(blast) => {
                        let mut text = blast.summary;
                        if !blast.impacted.is_empty() {
                            text.push_str("\n\nImpacted:\n");
                            for s in &blast.impacted { text.push_str("  • "); text.push_str(s); text.push('\n'); }
                        }
                        if let Some(hud) = self.hud.as_ref() { hud.set_diff_panel(true, &text); }
                    }
                    Err(e) => {
                        log::error!(":diff failed: {e}");
                        self.set_status(&format!("diff failed: {e}"));
                    }
                }
            }
```

Add an `Esc` handler to dismiss the diff panel — extend the `Mode::Normal` Esc / dismiss path so `Esc` clears `set_diff_panel(false, "")`.

- [ ] **Step 4: Smoke-test**

Make sure orthogonal is in a workspace whose focused tile is at e.g. `https://github.com/anthropics/orthogonal/...` and that local path is in config. `:diff` should populate the HUD diff panel. `Esc` dismisses.

- [ ] **Step 5: Commit**

```bash
git add crates/orthogonal-app/src/devtools_repo.rs crates/orthogonal-app/src/app.rs
git commit -m "feat(app): :diff resolves repo + shows mairu blast radius in HUD"
```

---

## Task 16: `:skill` — skill palette overlay

**Goal:** `:skill` calls `mairu.skill_list`, populates the existing suggestion-overlay UI with skill names + descriptions. Selecting one prints the chosen skill name to status (Phase 1 stops here — actually invoking the skill is mairu-agent-side work).

**Files:**
- Modify: `crates/orthogonal-app/src/app.rs`

- [ ] **Step 1: Reuse the suggestions list**

Add a tiny `SkillsOverlay` state to `App`:

```rust
    skill_overlay_active: bool,
    skill_overlay_items: Vec<orthogonal_mairu::client::Skill>,
    skill_overlay_index: usize,
```

Initialize all three to defaults in `App::new`.

- [ ] **Step 2: Handle `Action::OpenSkillPalette`**

```rust
            Action::OpenSkillPalette => {
                let Some(bridge) = self.bridge.as_ref() else { self.set_status("mairu bridge unavailable"); return };
                let Some(project) = self.current_project() else { self.set_status("set a project first: :project <name>"); return };
                let client = bridge.client.clone();
                let result = bridge.block_on(async move { client.skill_list(&project).await });
                match result {
                    Ok(list) => {
                        self.skill_overlay_items = list;
                        self.skill_overlay_index = 0;
                        self.skill_overlay_active = !self.skill_overlay_items.is_empty();
                        self.publish_skill_overlay_to_hud();
                    }
                    Err(e) => self.set_status(&format!("skill list failed: {e}")),
                }
            }
```

Add the helper:

```rust
    fn publish_skill_overlay_to_hud(&self) {
        let Some(hud) = self.hud.as_ref() else { return };
        let items: Vec<orthogonal_core::hud::SuggestionItem> = self.skill_overlay_items.iter().enumerate().map(|(i, s)| {
            orthogonal_core::hud::SuggestionItem {
                title: s.name.clone(),
                url: s.description.clone(),
                selected: i == self.skill_overlay_index,
            }
        }).collect();
        hud.set_suggestions(items);
        hud.set_suggestions_visible(self.skill_overlay_active);
    }
```

> Note: `Hud::set_suggestions` and `set_suggestions_visible` exist (the `SuggestionItem` Slint struct already exists in `hud.slint`). If method names differ, read `hud.rs` for the exact existing signatures.

- [ ] **Step 3: Wire palette navigation in Normal mode**

When `skill_overlay_active`, intercept `j`/`k`/`Enter`/`Esc` keys before the normal-mode dispatch:

```rust
    fn handle_skill_overlay_key(&mut self, c: char, is_enter: bool, is_esc: bool) -> bool {
        if !self.skill_overlay_active { return false; }
        if is_esc {
            self.skill_overlay_active = false;
            self.publish_skill_overlay_to_hud();
            return true;
        }
        if is_enter {
            if let Some(s) = self.skill_overlay_items.get(self.skill_overlay_index) {
                self.set_status(&format!("skill: {} (run via agent tile)", s.name));
            }
            self.skill_overlay_active = false;
            self.publish_skill_overlay_to_hud();
            return true;
        }
        if c == 'j' { self.skill_overlay_index = (self.skill_overlay_index + 1).min(self.skill_overlay_items.len().saturating_sub(1)); self.publish_skill_overlay_to_hud(); return true; }
        if c == 'k' { self.skill_overlay_index = self.skill_overlay_index.saturating_sub(1); self.publish_skill_overlay_to_hud(); return true; }
        false
    }
```

Call this at the top of the keyboard event handler (before passing to InputRouter).

- [ ] **Step 4: Commit**

Run: `cargo build --workspace` first to confirm no errors.

```bash
git add crates/orthogonal-app/src/app.rs
git commit -m "feat(app): :skill opens mairu skill palette as suggestion overlay"
```

---

## Task 17: `:inspect` — element inspector via JS shim

**Goal:** `:inspect` enters a mode where mouse-over highlights elements, click captures `tag/id/classes/attrs/computed-styles` to the HUD inspect line.

**Files:**
- Create: `crates/orthogonal-core/src/devtools.rs`
- Modify: `crates/orthogonal-core/src/lib.rs`
- Modify: `crates/orthogonal-app/src/app.rs`

- [ ] **Step 1: JS shim + types**

Create `crates/orthogonal-core/src/devtools.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const INSPECTOR_INSTALL_SCRIPT: &str = r#"(function() {
    if (window.__orthogonal_inspector__) return JSON.stringify({ ok: true });
    const state = { hovered: null, outline: null };
    function clearOutline() { if (state.outline) { state.outline.remove(); state.outline = null; } }
    function drawOutline(el) {
        clearOutline();
        const r = el.getBoundingClientRect();
        const o = document.createElement('div');
        o.id = '__orthogonal_inspector_outline__';
        o.style.cssText = `position:fixed;pointer-events:none;z-index:2147483647;
            left:${r.left}px;top:${r.top}px;width:${r.width}px;height:${r.height}px;
            outline:2px solid #ffcc00;background:rgba(255,204,0,0.08);`;
        document.body.appendChild(o);
        state.outline = o;
    }
    function describe(el) {
        const cs = window.getComputedStyle(el);
        const attrs = {};
        for (const a of el.attributes) attrs[a.name] = a.value;
        return {
            tag: el.tagName.toLowerCase(),
            id: el.id || null,
            classes: Array.from(el.classList),
            attrs: attrs,
            text: (el.textContent || '').trim().slice(0, 80),
            box: { x: Math.round(el.getBoundingClientRect().x), y: Math.round(el.getBoundingClientRect().y),
                   w: Math.round(el.getBoundingClientRect().width), h: Math.round(el.getBoundingClientRect().height) },
            font: cs.fontFamily + ' ' + cs.fontSize,
            color: cs.color,
            background: cs.backgroundColor,
        };
    }
    function onMove(e) { state.hovered = e.target; drawOutline(e.target); }
    function onClick(e) {
        e.preventDefault(); e.stopPropagation();
        const data = describe(e.target);
        window.__orthogonal_last_inspect = data;
        return false;
    }
    document.addEventListener('mousemove', onMove, true);
    document.addEventListener('click', onClick, true);
    window.__orthogonal_inspector__ = { uninstall: () => {
        document.removeEventListener('mousemove', onMove, true);
        document.removeEventListener('click', onClick, true);
        clearOutline();
        delete window.__orthogonal_inspector__;
    }};
    return JSON.stringify({ ok: true });
})()"#;

pub const INSPECTOR_POLL_SCRIPT: &str = r#"(function() {
    const d = window.__orthogonal_last_inspect;
    if (!d) return JSON.stringify({ ready: false });
    window.__orthogonal_last_inspect = null;
    return JSON.stringify({ ready: true, data: d });
})()"#;

pub const INSPECTOR_UNINSTALL_SCRIPT: &str = r#"(function() {
    if (window.__orthogonal_inspector__) window.__orthogonal_inspector__.uninstall();
    return JSON.stringify({ ok: true });
})()"#;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct InspectInfo {
    pub tag: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub font: String,
}

impl InspectInfo {
    pub fn one_liner(&self) -> String {
        let mut s = format!("<{}", self.tag);
        if let Some(id) = &self.id { s.push_str(&format!(" #{id}")); }
        for c in &self.classes { s.push_str(&format!(" .{c}")); }
        s.push_str("> ");
        s.push_str(&self.text);
        if !self.color.is_empty() {
            s.push_str(&format!("  [color: {} bg: {} font: {}]", self.color, self.background, self.font));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_liner_format() {
        let i = InspectInfo {
            tag: "a".into(), id: Some("login".into()), classes: vec!["btn".into(), "primary".into()],
            text: "Sign in".into(), color: "rgb(0,0,0)".into(), background: "rgb(255,255,255)".into(),
            font: "Helvetica 14px".into(),
        };
        let s = i.one_liner();
        assert!(s.contains("<a #login .btn .primary>"));
        assert!(s.contains("Sign in"));
        assert!(s.contains("color: rgb(0,0,0)"));
    }
}
```

Edit `crates/orthogonal-core/src/lib.rs` to add `pub mod devtools;`.

- [ ] **Step 2: Wire `Action::EnterInspect` and a periodic poll**

In `App`:

```rust
    inspect_active: bool,
    last_inspect_poll: std::time::Instant,
```

In `App::new`: `inspect_active: false, last_inspect_poll: std::time::Instant::now(),`.

Handler:

```rust
            Action::EnterInspect => {
                let Some(focused) = self.layout.focused() else { return };
                let Some(engine) = self.engine.as_ref() else { return };
                engine.evaluate_js(focused, orthogonal_core::devtools::INSPECTOR_INSTALL_SCRIPT, Box::new(|_| {}));
                self.inspect_active = true;
                if let Some(hud) = self.hud.as_ref() { hud.set_inspect_info(true, "inspect: hover an element, click to capture, Esc to exit"); }
            }
            Action::ExitToNormal if self.inspect_active => {
                let Some(focused) = self.layout.focused() else { self.inspect_active = false; return };
                let Some(engine) = self.engine.as_ref() else { self.inspect_active = false; return };
                engine.evaluate_js(focused, orthogonal_core::devtools::INSPECTOR_UNINSTALL_SCRIPT, Box::new(|_| {}));
                self.inspect_active = false;
                if let Some(hud) = self.hud.as_ref() { hud.set_inspect_info(false, ""); }
            }
```

> Note: `ExitToNormal` is already an Action variant — guard the inspect-uninstall branch with `self.inspect_active` and let the existing handler still run.

Periodic poll (call from `UserEvent::ServoTick`):

```rust
    fn poll_inspect(&mut self) {
        if !self.inspect_active { return; }
        if self.last_inspect_poll.elapsed() < std::time::Duration::from_millis(100) { return; }
        self.last_inspect_poll = std::time::Instant::now();
        let Some(focused) = self.layout.focused() else { return };
        let Some(engine) = self.engine.as_ref() else { return };
        // Use a channel to surface the result from the JS callback to the App thread.
        let (tx, rx) = std::sync::mpsc::channel();
        engine.evaluate_js(focused, orthogonal_core::devtools::INSPECTOR_POLL_SCRIPT, Box::new(move |r| {
            let _ = tx.send(r);
        }));
        // Drain on the next frame — store rx for one-shot pickup.
        if let Ok(Ok(s)) = rx.try_recv() {
            let trimmed = s.trim_matches('"');
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if v["ready"].as_bool().unwrap_or(false) {
                    if let Ok(info) = serde_json::from_value::<orthogonal_core::devtools::InspectInfo>(v["data"].clone()) {
                        if let Some(hud) = self.hud.as_ref() {
                            hud.set_inspect_info(true, &info.one_liner());
                        }
                    }
                }
            }
        }
    }
```

Call `self.poll_inspect();` at the top of the ServoTick handler.

- [ ] **Step 3: Smoke-test**

`cargo run -p orthogonal-app`. Open a content-rich page. `:inspect` → hover should outline elements; click captures one; HUD shows `<tag #id .class> text [color: ... bg: ... font: ...]`. `Esc` exits and removes the outline.

- [ ] **Step 4: Commit**

```bash
git add crates/orthogonal-core/src/devtools.rs crates/orthogonal-core/src/lib.rs crates/orthogonal-app/src/app.rs
git commit -m "feat(devtools): :inspect with hover-outline + element capture HUD line"
```

---

## Task 18: `:console` — JS console panel

**Goal:** `:console` toggles a console panel in the HUD. While visible, typed lines are evaluated against the focused tile's webview; results append to a scrollback buffer (cap 200 lines).

**Files:**
- Modify: `crates/orthogonal-core/src/devtools.rs`
- Modify: `crates/orthogonal-core/src/input.rs` (new sub-mode)
- Modify: `crates/orthogonal-app/src/app.rs`

- [ ] **Step 1: Add `ConsoleHistory` struct + tests**

Append to `crates/orthogonal-core/src/devtools.rs`:

```rust
pub struct ConsoleHistory {
    lines: std::collections::VecDeque<String>,
    cap: usize,
}

impl ConsoleHistory {
    pub fn new(cap: usize) -> Self { Self { lines: std::collections::VecDeque::with_capacity(cap), cap } }
    pub fn push(&mut self, line: String) {
        if self.lines.len() == self.cap { self.lines.pop_front(); }
        self.lines.push_back(line);
    }
    pub fn lines(&self) -> Vec<String> { self.lines.iter().cloned().collect() }
}

#[cfg(test)]
mod console_tests {
    use super::*;
    #[test]
    fn ring_buffer_drops_oldest() {
        let mut h = ConsoleHistory::new(3);
        h.push("a".into()); h.push("b".into()); h.push("c".into()); h.push("d".into());
        assert_eq!(h.lines(), vec!["b", "c", "d"]);
    }
}
```

Run: `cargo test -p orthogonal-core --lib devtools`
Expected: 2 passed.

- [ ] **Step 2: Add a console sub-mode**

In `crates/orthogonal-core/src/input.rs`, add a new `Mode` variant:

```rust
    Console { input: String },
```

Handle in `InputRouter::handle`:

```rust
            Mode::Console { .. } => self.handle_console(event),
```

Add the handler:

```rust
    fn handle_console(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        match event.key {
            CoreKey::Escape => { self.mode = Mode::Normal; vec![Action::ToggleConsole] }
            CoreKey::Enter => {
                let line = if let Mode::Console { input } = &self.mode { input.clone() } else { String::new() };
                if let Mode::Console { input } = &mut self.mode { input.clear(); }
                vec![Action::ConsoleEval(line)]
            }
            CoreKey::Backspace => {
                if let Mode::Console { input } = &mut self.mode { input.pop(); }
                vec![Action::ConsoleInputChanged]
            }
            CoreKey::Char(c) => {
                if let Mode::Console { input } = &mut self.mode { input.push(c); }
                vec![Action::ConsoleInputChanged]
            }
            _ => vec![],
        }
    }
```

Add the new `Action` variants:

```rust
    ConsoleEval(String),
    ConsoleInputChanged,
```

Update `parse_command` `Some("console")` branch to enter Console mode by returning `Action::ToggleConsole` (the App handler will set the mode via a new `enter_console_mode` accessor on InputRouter):

```rust
    pub fn enter_console_mode(&mut self) {
        self.mode = Mode::Console { input: String::new() };
    }

    pub fn exit_console_mode(&mut self) {
        self.mode = Mode::Normal;
    }
```

Add input tests:

```rust
    #[test]
    fn console_mode_builds_input() {
        let mut router = InputRouter::new();
        router.enter_console_mode();
        router.handle(&key('1'));
        router.handle(&key('+'));
        router.handle(&key('1'));
        if let Mode::Console { input } = router.mode() { assert_eq!(input, "1+1"); } else { panic!(); }
    }

    #[test]
    fn console_enter_emits_eval_with_buffer() {
        let mut router = InputRouter::new();
        router.enter_console_mode();
        for c in "2*3".chars() { router.handle(&key(c)); }
        let actions = router.handle(&special(CoreKey::Enter));
        assert_eq!(actions, vec![Action::ConsoleEval("2*3".into())]);
    }
```

Run: `cargo test -p orthogonal-core --lib input`
Expected: all green.

- [ ] **Step 3: App-side wiring**

Add to `App`:

```rust
    console_history: orthogonal_core::devtools::ConsoleHistory,
    console_visible: bool,
```

In `App::new`: `console_history: orthogonal_core::devtools::ConsoleHistory::new(200), console_visible: false,`.

Handlers:

```rust
            Action::ToggleConsole => {
                self.console_visible = !self.console_visible;
                if self.console_visible { self.input.enter_console_mode(); } else { self.input.exit_console_mode(); }
                if let Some(hud) = self.hud.as_ref() {
                    hud.set_console_visible(self.console_visible);
                    hud.set_console_lines(self.console_history.lines());
                    hud.set_console_input("");
                }
            }
            Action::ConsoleInputChanged => {
                if let Mode::Console { input } = self.input.mode() {
                    if let Some(hud) = self.hud.as_ref() { hud.set_console_input(input); }
                }
            }
            Action::ConsoleEval(expr) => {
                let Some(focused) = self.layout.focused() else { return };
                let Some(engine) = self.engine.as_ref() else { return };
                let prompt = format!("> {expr}");
                self.console_history.push(prompt);
                let (tx, rx) = std::sync::mpsc::channel();
                engine.evaluate_js(focused, &expr, Box::new(move |r| { let _ = tx.send(r); }));
                // Best-effort sync drain: try once, fall back to a deferred pickup channel
                if let Ok(r) = rx.try_recv() {
                    let s = match r { Ok(v) => v, Err(e) => format!("error: {e}") };
                    self.console_history.push(s);
                }
                if let Some(hud) = self.hud.as_ref() {
                    hud.set_console_lines(self.console_history.lines());
                    hud.set_console_input("");
                }
            }
```

> Note: Servo's `evaluate_javascript` callback may fire on a later tick. For Phase 1, accept the "best-effort sync" semantics — most simple expressions resolve in the same tick. A follow-up task can deferred-channel results properly.

- [ ] **Step 4: Smoke-test**

`cargo run -p orthogonal-app`, open any page, `:console`, type `2+2`, Enter → `4` appears (or "Number(4)" / similar Servo serialization). `Esc` closes.

- [ ] **Step 5: Commit**

```bash
git add crates/orthogonal-core/src/devtools.rs crates/orthogonal-core/src/input.rs crates/orthogonal-app/src/app.rs
git commit -m "feat(devtools): :console JS eval panel with scrollback ring buffer"
```

---

## Task 19: `:network` — request log per tile

**Goal:** A per-tile ring buffer of recent network requests. `:network` opens a tile rendering the current focused tile's buffer as JSON-pretty-printed text. If Servo's network observer isn't available, fall back to "DevTools mode unavailable on this Servo build".

**Files:**
- Modify: `crates/orthogonal-core/src/devtools.rs`
- Modify: `crates/orthogonal-app/src/app.rs`
- Possibly: `crates/orthogonal-servo/src/lib.rs` (depends on Servo API surface)

- [ ] **Step 1: Ring buffer + types + tests**

Append to `crates/orthogonal-core/src/devtools.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetEntry {
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub duration_ms: u64,
    pub bytes: Option<u64>,
    pub started_at_ms: u64,
}

pub struct NetworkBuffer {
    entries: std::collections::VecDeque<NetEntry>,
    cap: usize,
}

impl NetworkBuffer {
    pub fn new(cap: usize) -> Self { Self { entries: std::collections::VecDeque::with_capacity(cap), cap } }
    pub fn push(&mut self, e: NetEntry) {
        if self.entries.len() == self.cap { self.entries.pop_front(); }
        self.entries.push_back(e);
    }
    pub fn entries(&self) -> Vec<NetEntry> { self.entries.iter().cloned().collect() }
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("method  status  ms      bytes        url\n");
        for e in self.entries.iter().rev() {
            out.push_str(&format!(
                "{:6}  {:6}  {:6}  {:>10}  {}\n",
                e.method,
                e.status.map(|s| s.to_string()).unwrap_or_else(|| "-".into()),
                e.duration_ms,
                e.bytes.map(|b| b.to_string()).unwrap_or_else(|| "-".into()),
                e.url,
            ));
        }
        out
    }
}

#[cfg(test)]
mod net_tests {
    use super::*;
    #[test]
    fn buffer_caps_and_renders() {
        let mut b = NetworkBuffer::new(2);
        b.push(NetEntry { method: "GET".into(), url: "https://a/".into(), status: Some(200), duration_ms: 10, bytes: Some(1024), started_at_ms: 0 });
        b.push(NetEntry { method: "POST".into(), url: "https://b/".into(), status: Some(201), duration_ms: 25, bytes: Some(2048), started_at_ms: 1 });
        b.push(NetEntry { method: "GET".into(), url: "https://c/".into(), status: None, duration_ms: 5, bytes: None, started_at_ms: 2 });
        assert_eq!(b.entries().len(), 2);
        let r = b.render();
        assert!(r.contains("https://b/"));
        assert!(r.contains("https://c/"));
        assert!(!r.contains("https://a/"));
    }
}
```

Run: `cargo test -p orthogonal-core --lib devtools`
Expected: 3 passed.

- [ ] **Step 2: Investigate Servo's network observer**

Read `crates/orthogonal-servo/src/lib.rs` and the Servo API docs to determine the exact network-listener API in this Servo version. Possible APIs:

- `WebView::set_devtools_attached(true)` + a delegate hook
- A `NetworkListener` trait in `servo::script_traits` or `servo::net`

If the API exists: extend `Engine` with `register_network_listener(view_id: ViewId, sink: mpsc::Sender<NetEntry>)`. Document exactly which Servo type/method you used in a comment in the function.

If the API does not exist: skip Step 3 implementation and instead make Step 4 always render the fallback message.

- [ ] **Step 3: Wire the listener (if available)**

In `App`:

```rust
    network_buffers: std::collections::HashMap<ViewId, orthogonal_core::devtools::NetworkBuffer>,
    network_rx: std::sync::mpsc::Receiver<(ViewId, orthogonal_core::devtools::NetEntry)>,
    network_tx: std::sync::mpsc::Sender<(ViewId, orthogonal_core::devtools::NetEntry)>,
```

Initialize the channel + empty map in `App::new`. When a tile is created, register the listener.

In `UserEvent::ServoTick`, drain `network_rx` into `network_buffers`.

- [ ] **Step 4: Handle `Action::OpenNetworkTile`**

```rust
            Action::OpenNetworkTile => {
                let Some(focused) = self.layout.focused() else { return };
                let buffer = self.network_buffers.get(&focused);
                let body = match buffer {
                    Some(b) if !b.entries().is_empty() => b.render(),
                    Some(_) => "no requests captured yet".into(),
                    None => "DevTools mode unavailable on this Servo build".into(),
                };
                // Open as a data: URL tile so it renders as plain text.
                let html = format!(
                    "<!doctype html><meta charset=utf-8><title>:network</title><pre style='font-family:ui-monospace,monospace;font-size:11px;padding:8px'>{}</pre>",
                    html_escape(&body)
                );
                let url = format!("data:text/html;base64,{}", base64_encode(&html));
                if let Some(focused) = self.layout.focused() {
                    if let Some(engine) = self.engine.as_mut() {
                        let new_id = self.views.create(&url);
                        self.layout.split(focused, SplitDirection::Horizontal, new_id);
                        engine.create_tile(new_id, &url);
                        self.layout.set_focused(new_id);
                    }
                }
                self.publish_tiles_snapshot();
            }
```

Add small helpers (or import a crate — `base64 = "0.22"` if not already present):

```rust
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
fn base64_encode(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}
```

Add `base64 = "0.22"` to `crates/orthogonal-app/Cargo.toml` if needed.

- [ ] **Step 5: Smoke-test**

`cargo run -p orthogonal-app`, open a fetch-heavy page (e.g. `https://news.ycombinator.com`). `:network` should open a side tile with a captured request log (or the fallback message if Servo's API doesn't expose one in this build).

- [ ] **Step 6: Commit**

```bash
git add crates/orthogonal-core/src/devtools.rs crates/orthogonal-app
git commit -m "feat(devtools): :network opens per-tile request log (with fallback)"
```

---

## Task 20: Final integration smoke test + spec update

**Goal:** Run through the spec's manual-checklist golden paths end-to-end. Update spec status. Commit.

- [ ] **Step 1: Manual checklist (run from `/Users/enekosarasola/orthogonal`)**

Pre-flight:
- `mairu context-server -p 8788 &` (in another terminal)
- `cargo run -p orthogonal-app`

Checklist (write a one-line note next to each PASS/FAIL):

- [ ] Open agent tile with `:agent` — tile loads `http://127.0.0.1:8788/agent?…` (404 from mairu is OK if mairu doesn't have the route yet; orthogonal's part is the request).
- [ ] `:project mairu` shows `[proj:mairu]` in HUD.
- [ ] `:project --workspace orthogonal` followed by `:project --clear` shows `[proj:orthogonal]`.
- [ ] `:scrape https://news.ycombinator.com` opens a reader tile (assuming mairu's `/scrape/web` is wired).
- [ ] `:diff` against a known repo URL with mapping in config produces blast-radius panel.
- [ ] `:skill` opens skill palette; j/k navigates; Enter prints status; Esc closes.
- [ ] `:inspect` outlines elements on hover, click captures HUD info, Esc removes outline.
- [ ] `:console` opens panel; `2+2` prints a result; Esc closes.
- [ ] `:network` opens log tile (or fallback message).
- [ ] `[mairu]` indicator turns red within 15s after killing the daemon, green within 15s after restarting.
- [ ] `cargo test --workspace` exits 0.

- [ ] **Step 2: Update spec status**

Edit `docs/superpowers/specs/2026-04-19-orthogonal-mairu-phase-1-design.md` line 3:

```markdown
**Status:** Implemented (Phase 1 complete YYYY-MM-DD)
```

(Use the actual completion date.)

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-04-19-orthogonal-mairu-phase-1-design.md
git commit -m "docs(spec): mark Orthogonal × Mairu Phase 1 as implemented"
```

---

## Spec coverage check

| Spec section | Implemented in task |
|---|---|
| §2 architecture (new crate, two pieces) | T1, T5 |
| §3.1 MairuClient (typed methods) | T4 |
| §3.2 Axum tool server (`/tiles`, `/tiles/focused`, `/tiles/:id`, `/health`, bearer auth, descriptor file) | T3, T5 |
| §3.3 Project tagging (workspace + tile override + HUD) | T6, T7, T8, T9, T11, T12 |
| §3.4 Commands (`:agent`, `:project`, `:scrape`, `:diff`, `:skill`, `:inspect`, `:console`, `:network`) | T10 (parse), T12 (`:project`), T13 (`:agent`), T14 (`:scrape`), T15 (`:diff`), T16 (`:skill`), T17 (`:inspect`), T18 (`:console`), T19 (`:network`) |
| §3.5 DevTools-lite implementation | T17, T18, T19 |
| §4 data flows | T13 (agent flow), T14 (scrape), T15 (diff) |
| §5 error handling (mairu down, auth failure, project unset, network observer fallback) | T12 (down), T3+T5 (auth), T14/T15/T16 (project unset), T19 (network fallback) |
| §6 testing strategy | unit tests in T2/T3/T4/T5/T6/T7/T8/T10/T15/T17/T18/T19; integration in T11/T12/T20 manual |
| §7 mairu-side work | explicitly out-of-scope for this plan (referenced in T13 smoke note) |
| §8 sequencing | T1–T5 = step 1; T6–T10 = step 2 + foundation; T11–T12 = steps 3–4; T13 = step 4; T14–T16 = step 5; T17 = step 6; T18 = step 7; T19 = step 8 |
