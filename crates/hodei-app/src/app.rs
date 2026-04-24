use std::collections::HashMap;
use std::sync::mpsc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowId};
use glow::HasContext;

use hodei_core::bookmarks::BookmarkManager;
use hodei_core::compositor::Compositor;
use hodei_core::config::Config;
use hodei_core::db;
use hodei_core::hint;
use hodei_core::history::HistoryManager;
use hodei_core::search;
use hodei_core::hud::Hud;
use hodei_core::input::{Action, InputRouter, Mode};
use hodei_core::layout::BspLayout;
use hodei_core::suggest::{self, Suggestion, SuggestionSource};
use hodei_core::types::*;
use hodei_core::view::ViewManager;
use hodei_core::workspace::WorkspaceManager;
use hodei_servo::Engine;

use crate::UserEvent;

/// Wraps EventLoopProxy as a Servo EventLoopWaker.
struct WinitWaker {
    proxy: EventLoopProxy<UserEvent>,
}

impl hodei_servo::ServoEventLoopWaker for WinitWaker {
    fn clone_box(&self) -> Box<dyn hodei_servo::ServoEventLoopWaker> {
        Box::new(WinitWaker {
            proxy: self.proxy.clone(),
        })
    }

    fn wake(&self) {
        let _ = self.proxy.send_event(UserEvent::ServoTick);
    }
}

pub struct App {
    proxy: EventLoopProxy<UserEvent>,
    config: Config,
    window: Option<Window>,
    engine: Option<Engine>,
    hud: Option<Hud>,
    compositor: Option<Compositor>,
    layout: BspLayout,
    views: ViewManager,
    input: InputRouter,
    modifiers: winit::keyboard::ModifiersState,
    mouse_position: (f64, f64),
    // Hint mode state
    hint_elements: Vec<HintElement>,
    hint_labels: Vec<String>,
    hint_result_tx: mpsc::Sender<String>,
    hint_result_rx: mpsc::Receiver<String>,
    // Managers
    workspace: Option<WorkspaceManager>,
    history: Option<HistoryManager>,
    bookmarks: Option<BookmarkManager>,
    // Suggestion state
    suggestions: Vec<Suggestion>,
    suggestion_index: usize,
    last_search_query: String,
    // Search state
    search_result_tx: mpsc::Sender<search::SearchResult>,
    search_result_rx: mpsc::Receiver<search::SearchResult>,
    current_search_result: search::SearchResult,
    // Zoom state (per tile zoom level)
    tile_zoom_levels: HashMap<ViewId, f32>,
    // Last focused tile for swap
    last_focused: Option<ViewId>,
    // Status text per tile (hover URL)
    status_texts: HashMap<ViewId, String>,
    // Theme toggle state
    theme_dark: bool,
    // Keyboard shortcuts modal state
    show_shortcuts: bool,
    // Set by notify_new_frame_ready; cleared after paint_tile is called
    pending_paint: bool,
    // DevTools WebSocket bridge
    devtools_bridge: Option<crate::devtools_bridge::DevToolsBridge>,
    // DevTools server status (tcp_port, token)
    devtools_status: Option<(u16, String)>,
    // Last time HODEI_SCREENSHOT wrote a PNG (for 1Hz throttle)
    last_screenshot: Option<std::time::Instant>,
    // Per-view load status (drives loading spinner in HUD)
    tile_load_status: HashMap<ViewId, TileLoadStatus>,
    // Per-view history availability (drives back/forward icon dimming)
    tile_nav_availability: HashMap<ViewId, (bool, bool)>,
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let config_path = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("hodei")
            .join("config.toml");
        let config = Config::load(&config_path);
        log::info!("App::new: config loaded from {:?}", config_path);
        let input = InputRouter::with_overrides(&config.keybindings);
        let (hint_result_tx, hint_result_rx) = mpsc::channel();
        let (search_result_tx, search_result_rx) = mpsc::channel();

        log::info!("App::new: initialized");
        Self {
            proxy,
            config,
            window: None,
            engine: None,
            hud: None,
            compositor: None,
            layout: BspLayout::new(Rect::default()),
            views: ViewManager::new(),
            input,
            modifiers: winit::keyboard::ModifiersState::empty(),
            mouse_position: (0.0, 0.0),
            hint_elements: Vec::new(),
            hint_labels: Vec::new(),
            hint_result_tx,
            hint_result_rx,
            workspace: None,
            history: None,
            bookmarks: None,
            suggestions: Vec::new(),
            suggestion_index: 0,
            last_search_query: String::new(),
            search_result_tx,
            search_result_rx,
            current_search_result: search::SearchResult { index: 0, count: 0 },
            tile_zoom_levels: HashMap::new(),
            last_focused: None,
            status_texts: HashMap::new(),
            theme_dark: false,
            show_shortcuts: false,
            pending_paint: false,
            devtools_bridge: None,
            devtools_status: None,
            last_screenshot: None,
            tile_load_status: HashMap::new(),
            tile_nav_availability: HashMap::new(),
        }
    }

    fn maybe_dump_screenshot(&mut self, gl: &glow::Context, width: u32, height: u32) {
        // Throttle to ~1Hz.
        let now = std::time::Instant::now();
        if let Some(prev) = self.last_screenshot {
            if now.duration_since(prev) < std::time::Duration::from_secs(1) {
                log::trace!("maybe_dump_screenshot: throttled");
                return;
            }
        }
        self.last_screenshot = Some(now);
        log::debug!("maybe_dump_screenshot: capturing {}x{}", width, height);

        let pixel_count = (width as usize) * (height as usize) * 4;
        let mut buf = vec![0u8; pixel_count];
        unsafe {
            gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
        }

        // glReadPixels returns pixels bottom-up; flip vertically for PNG.
        let row_bytes = (width as usize) * 4;
        let mut flipped = vec![0u8; pixel_count];
        for y in 0..(height as usize) {
            let src = y * row_bytes;
            let dst = (height as usize - 1 - y) * row_bytes;
            flipped[dst..dst + row_bytes].copy_from_slice(&buf[src..src + row_bytes]);
        }

        std::thread::spawn(move || {
            let out_dir = std::path::Path::new("target").join("screenshots");
            if let Err(e) = std::fs::create_dir_all(&out_dir) {
                log::warn!("screenshot: mkdir failed: {}", e);
                return;
            }
            let path = out_dir.join("latest.png");
            let img = match image::RgbaImage::from_raw(width, height, flipped) {
                Some(i) => i,
                None => {
                    log::warn!("screenshot: RgbaImage::from_raw failed");
                    return;
                }
            };
            if let Err(e) = img.save(&path) {
                log::warn!("screenshot: save failed: {}", e);
            } else {
                log::info!("screenshot: saved to {}", path.display());
            }
        });
    }

    fn dispatch_actions(&mut self, actions: Vec<Action>) {
        for action in &actions {
            log::debug!("dispatch_actions: {:?}", action);
        }
        for action in actions {
            match action {
                Action::FocusNeighbor(dir) => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(neighbor) = self.layout.focus_neighbor(focused, dir) {
                            log::info!("FocusNeighbor: {:?} -> {:?}", focused, neighbor);
                            self.layout.set_focused(neighbor);
                            self.update_hud();
                        } else {
                            log::debug!("FocusNeighbor: no neighbor in direction {:?} from {:?}", dir, focused);
                        }
                    }
                }
                Action::SplitView(dir) => {
                    if let Some(focused) = self.layout.focused() {
                        let new_id = self.views.create("about:blank");
                        log::info!("SplitView: focused={:?} new_id={:?} dir={:?}", focused, new_id, dir);
                        self.layout.split(focused, dir, new_id);
                        let (w, h) = self.tile_size(new_id);
                        if let Some(engine) = &mut self.engine {
                            engine.create_tile(new_id, "about:blank", w, h);
                        }
                        self.resize_all_tiles();
                        // Let Servo finish laying out both the new and the
                        // re-sized existing tile before the first blit so we
                        // don't flash a stale framebuffer into the new region.
                        if let Some(engine) = &mut self.engine {
                            engine.spin();
                        }
                        self.pending_paint = true;
                        self.update_hud();
                        self.request_redraw();
                    }
                }
                Action::CloseView => {
                    if let Some(focused) = self.layout.focused() {
                        log::info!("CloseView: {:?}", focused);
                        self.layout.close(focused);
                        self.views.remove(focused);
                        self.tile_load_status.remove(&focused);
                        self.tile_nav_availability.remove(&focused);
                        self.tile_zoom_levels.remove(&focused);
                        self.status_texts.remove(&focused);
                        if self.last_focused == Some(focused) {
                            self.last_focused = None;
                        }
                        if let Some(engine) = &mut self.engine {
                            engine.destroy_tile(focused);
                        }
                        self.resize_all_tiles();
                        if let Some(engine) = &mut self.engine { engine.spin(); }
                        self.pending_paint = true;
                        self.update_hud();
                        self.request_redraw();
                    }
                }
                Action::ResizeSplit(dir, delta) => {
                    if let Some(focused) = self.layout.focused() {
                        let signed_delta = match dir {
                            Direction::Right | Direction::Down => delta,
                            Direction::Left | Direction::Up => -delta,
                        };
                        log::debug!("ResizeSplit: focused={:?} dir={:?} delta={}", focused, dir, signed_delta);
                        self.layout.resize_split(focused, signed_delta);
                        self.resize_all_tiles();
                        if let Some(engine) = &mut self.engine { engine.spin(); }
                        self.pending_paint = true;
                        self.request_redraw();
                    }
                }
                Action::ForwardToServo(key_event) => {
                    if let Some(focused) = self.layout.focused() {
                        log::trace!("ForwardToServo: view={:?} key={:?}", focused, key_event.key);
                        if let Some(engine) = &self.engine {
                            engine.send_input(focused, key_event);
                        }
                    }
                }
                Action::Navigate(input) => {
                    let url = if !self.suggestions.is_empty() && self.suggestion_index < self.suggestions.len() {
                        self.suggestions[self.suggestion_index].url.clone()
                    } else if input.contains('.') || input.starts_with("http") {
                        if input.starts_with("http://") || input.starts_with("https://") {
                            input.clone()
                        } else {
                            format!("https://{}", input)
                        }
                    } else {
                        self.config.search_url(&input)
                    };

                    if let Some(focused) = self.layout.focused() {
                        log::info!("Navigate: view={:?} url={}", focused, url);
                        if let Some(engine) = &self.engine {
                            engine.navigate(focused, &url);
                        }
                        if let Some(view) = self.views.get_mut(focused) {
                            view.url = url;
                        }
                    }
                    self.suggestions.clear();
                    self.suggestion_index = 0;
                    self.update_hud();
                }
                Action::Back => {
                    if let Some(focused) = self.layout.focused() {
                        log::debug!("Back: view={:?}", focused);
                        if let Some(engine) = &self.engine {
                            engine.go_back(focused);
                        }
                    }
                }
                Action::Forward => {
                    if let Some(focused) = self.layout.focused() {
                        log::debug!("Forward: view={:?}", focused);
                        if let Some(engine) = &self.engine {
                            engine.go_forward(focused);
                        }
                    }
                }
                Action::Reload => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(view) = self.views.get(focused) {
                            let url = view.url.clone();
                            log::info!("Reload: view={:?} url={}", focused, url);
                            if let Some(engine) = &self.engine {
                                engine.navigate(focused, &url);
                            }
                        }
                    }
                }
                Action::EnterHintMode => {
                    if let Some(focused) = self.layout.focused() {
                        log::info!("EnterHintMode: view={:?}", focused);
                        if let Some(engine) = &self.engine {
                            let tx = self.hint_result_tx.clone();
                            engine.evaluate_js(
                                focused,
                                hint::HINT_QUERY_SCRIPT,
                                Box::new(move |result| {
                                    if let Ok(json) = result {
                                        let _ = tx.send(json);
                                    }
                                }),
                            );
                        }
                    }
                }
                Action::ActivateHint(label) => {
                    log::info!("ActivateHint: label={}", label);
                    if let Some(idx) = self.hint_labels.iter().position(|l| l == &label) {
                        if let Some(elem) = self.hint_elements.get(idx) {
                            if let Some(focused) = self.layout.focused() {
                                if let Some(engine) = &self.engine {
                                    engine.send_click(focused, elem.x as f32, elem.y as f32);
                                }
                            }
                        }
                    }
                    self.hint_elements.clear();
                    self.hint_labels.clear();
                    if let Some(hud) = &self.hud {
                        hud.clear_hints();
                    }
                    self.update_hud();
                }
                Action::HintCharTyped(c) => {
                    log::trace!("HintCharTyped: {}", c);
                    self.update_hint_display();
                }
                Action::EnterInsert | Action::EnterCommand | Action::ExitToNormal => {
                    self.update_hud();
                }
                Action::EnterSearch => {
                    log::info!("EnterSearch");
                    if let Some(hud) = &self.hud {
                        hud.set_search_visible(true);
                        hud.set_search_text("");
                        hud.request_redraw();
                    }
                    self.update_hud();
                }
                Action::SearchQueryChanged(query) => {
                    log::trace!("SearchQueryChanged: {}", query);
                    self.last_search_query = query.clone();
                    if let Some(hud) = &self.hud {
                        hud.set_search_text(&query);
                        hud.request_redraw();
                    }
                    if !query.is_empty() {
                        self.inject_search_init(&query);
                    }
                    self.update_hud();
                }
                Action::SearchNext => {
                    log::debug!("SearchNext");
                    self.inject_search_navigate(1);
                }
                Action::SearchPrev => {
                    log::debug!("SearchPrev");
                    self.inject_search_navigate(-1);
                }
                Action::SearchClear => {
                    log::debug!("SearchClear");
                    self.last_search_query.clear();
                    self.current_search_result = search::SearchResult { index: 0, count: 0 };
                    self.inject_search_clear();
                    if let Some(hud) = &self.hud {
                        hud.set_search_visible(false);
                        hud.set_search_info("");
                        hud.request_redraw();
                    }
                    self.update_hud();
                }
                Action::SaveSession => {
                    log::info!("SaveSession");
                    let (nodes, focused) = self.layout.serialize();
                    let tiles = self.collect_tile_rows();
                    if let Some(workspace) = &mut self.workspace {
                        if let Err(e) = workspace.save_active(&nodes, &tiles, focused) {
                            log::error!("Failed to save: {}", e);
                        } else {
                            log::info!("Session saved");
                        }
                    }
                }
                Action::RestoreSession => {
                    log::info!("RestoreSession");
                    self.dispatch_actions(vec![Action::WorkspaceSwitch("default".to_string())]);
                }
                Action::Quit => {
                    log::info!("Quit");
                    self.autosave();
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
                Action::Bookmark(tags) => {
                    if let Some(bookmarks) = &self.bookmarks {
                        if let Some(focused) = self.layout.focused() {
                            if let Some(view) = self.views.get(focused) {
                                let tags_str = tags.as_deref().unwrap_or("");
                                let _ = bookmarks.add(&view.url, &view.title, tags_str);
                                log::info!("Bookmarked: {}", view.url);
                            }
                        }
                    }
                }
                Action::BookmarkDelete(url) => {
                    log::info!("BookmarkDelete: {}", url);
                    if let Some(bookmarks) = &self.bookmarks {
                        let _ = bookmarks.remove(&url);
                    }
                }
                Action::ShowBookmarks(query) => {
                    log::info!("ShowBookmarks: query='{}'", query);
                    if let Some(bookmarks) = &self.bookmarks {
                        let _results = bookmarks.search(&query, 20).unwrap_or_default();
                        log::info!("Bookmarks search: {} results", _results.len());
                    }
                }
                Action::ShowHistory(query) => {
                    log::info!("ShowHistory: query='{}'", query);
                    if let Some(history) = &self.history {
                        let _results = history.search(&query, 20).unwrap_or_default();
                        log::info!("History search: {} results", _results.len());
                    }
                }
                Action::CommandBufferChanged => {
                    log::trace!("CommandBufferChanged");
                    self.update_suggestions();
                    self.update_hud();
                }
                Action::SuggestionNext => {
                    log::trace!("SuggestionNext");
                    if !self.suggestions.is_empty() {
                        self.suggestion_index = (self.suggestion_index + 1) % self.suggestions.len();
                        self.update_suggestion_display();
                    }
                }
                Action::SuggestionPrev => {
                    log::trace!("SuggestionPrev");
                    if !self.suggestions.is_empty() {
                        self.suggestion_index = if self.suggestion_index == 0 {
                            self.suggestions.len() - 1
                        } else {
                            self.suggestion_index - 1
                        };
                        self.update_suggestion_display();
                    }
                }
                Action::ZoomIn => {
                    log::debug!("ZoomIn");
                    self.adjust_zoom(0.1);
                }
                Action::ZoomOut => {
                    log::debug!("ZoomOut");
                    self.adjust_zoom(-0.1);
                }
                Action::ZoomReset => {
                    if let Some(focused) = self.layout.focused() {
                        log::info!("ZoomReset: view={:?}", focused);
                        self.tile_zoom_levels.insert(focused, 1.0);
                        self.apply_zoom(focused, 1.0);
                    }
                }
                Action::YankUrl => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(view) = self.views.get(focused) {
                            self.copy_to_clipboard(&view.url);
                            log::info!("Yanked URL: {}", view.url);
                        }
                    }
                }
                Action::WorkspaceSwitch(name) => {
                    log::info!("WorkspaceSwitch: {}", name);
                    let (current_nodes, current_focused) = self.layout.serialize();
                    let current_tiles = self.collect_tile_rows();
                    if let Some(workspace) = &mut self.workspace {
                        match workspace.switch_to(&name, &current_nodes, &current_tiles, current_focused) {
                            Ok(Some(state)) => {
                                log::info!("WorkspaceSwitch: restoring workspace '{}' with {} tiles", name, state.tiles.len());
                                // Tear down current
                                for view_id in self.views.all_views() {
                                    if let Some(engine) = &mut self.engine {
                                        engine.destroy_tile(view_id);
                                    }
                                }
                                self.views = ViewManager::new();

                                // Restore workspace
                                self.layout = BspLayout::deserialize(self.layout_viewport(), &state.nodes, state.focused);
                                for tile in &state.tiles {
                                    self.views.create_with_id(tile.view_id, &tile.url);
                                    let (w, h) = self.tile_size(tile.view_id);
                                    if let Some(engine) = &mut self.engine {
                                        engine.create_tile(tile.view_id, &tile.url, w, h);
                                    }
                                }
                                self.resize_all_tiles();
                                self.update_hud();
                                self.request_redraw();
                            }
                            Ok(None) => {
                                log::info!("WorkspaceSwitch: creating new empty workspace '{}'", name);
                                // New empty workspace
                                for view_id in self.views.all_views() {
                                    if let Some(engine) = &mut self.engine {
                                        engine.destroy_tile(view_id);
                                    }
                                }
                                self.views = ViewManager::new();
                                self.layout = BspLayout::new(self.layout_viewport());

                                let startup_url = self.config.general.startup_url.clone();
                                let view_id = self.views.create(&startup_url);
                                self.layout.add_first_view(view_id);
                                let (w, h) = self.tile_size(view_id);
                                if let Some(engine) = &mut self.engine {
                                    engine.create_tile(view_id, &startup_url, w, h);
                                }
                                self.resize_all_tiles();
                                self.update_hud();
                                self.request_redraw();
                            }
                            Err(e) => log::error!("Failed to switch workspace: {}", e),
                        }
                    }
                }
                Action::WorkspaceNew(name) => {
                    log::info!("WorkspaceNew: {}", name);
                    self.dispatch_actions(vec![Action::WorkspaceSwitch(name)]);
                }
                Action::WorkspaceDelete(name) => {
                    log::info!("WorkspaceDelete: {}", name);
                    if let Some(workspace) = &mut self.workspace {
                        match workspace.delete(&name) {
                            Ok(true) => log::info!("Deleted workspace: {}", name),
                            Ok(false) => log::warn!("Cannot delete active workspace"),
                            Err(e) => log::error!("Failed to delete workspace: {}", e),
                        }
                    }
                }
                Action::WorkspaceList => {
                    log::info!("WorkspaceList");
                    if let Some(workspace) = &self.workspace {
                        if let Ok(list) = workspace.list() {
                            for info in &list {
                                log::info!("  {} ({} tiles)", info.name, info.tile_count);
                            }
                        }
                    }
                }
                Action::FocusNext => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(next) = self.layout.next_focus(focused) {
                            log::info!("FocusNext: {:?} -> {:?}", focused, next);
                            self.last_focused = Some(focused);
                            self.layout.set_focused(next);
                            self.update_hud();
                        }
                    }
                }
                Action::FocusPrev => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(prev) = self.layout.prev_focus(focused) {
                            log::info!("FocusPrev: {:?} -> {:?}", focused, prev);
                            self.last_focused = Some(focused);
                            self.layout.set_focused(prev);
                            self.update_hud();
                        }
                    }
                }
                Action::ScrollPageDown => {
                    if let Some(focused) = self.layout.focused() {
                        log::trace!("ScrollPageDown: view={:?}", focused);
                        self.inject_scroll(focused, "window.scrollBy(0, window.innerHeight / 2)");
                    }
                }
                Action::ScrollPageUp => {
                    if let Some(focused) = self.layout.focused() {
                        log::trace!("ScrollPageUp: view={:?}", focused);
                        self.inject_scroll(focused, "window.scrollBy(0, -window.innerHeight / 2)");
                    }
                }
                Action::ScrollToTop => {
                    if let Some(focused) = self.layout.focused() {
                        log::trace!("ScrollToTop: view={:?}", focused);
                        self.inject_scroll(focused, "window.scrollTo(0, 0)");
                    }
                }
                Action::ScrollToBottom => {
                    if let Some(focused) = self.layout.focused() {
                        log::trace!("ScrollToBottom: view={:?}", focused);
                        self.inject_scroll(focused, "window.scrollTo(0, document.body.scrollHeight)");
                    }
                }
                Action::HardReload => {
                    if let Some(focused) = self.layout.focused() {
                        log::info!("HardReload: view={:?}", focused);
                        if let Some(engine) = &self.engine {
                            engine.hard_reload(focused);
                        }
                    }
                }
                Action::PasteNavigate => {
                    if let Some(url) = self.read_clipboard() {
                        let url = Self::normalize_pasted_url(&url, &self.config);
                        if let Some(focused) = self.layout.focused() {
                            log::info!("PasteNavigate: view={:?} url={}", focused, url);
                            if let Some(engine) = &self.engine {
                                engine.navigate(focused, &url);
                            }
                            if let Some(view) = self.views.get_mut(focused) {
                                view.url = url;
                            }
                        }
                        self.update_hud();
                    } else {
                        log::warn!("PasteNavigate: clipboard empty or unreadable");
                    }
                }
                Action::PasteNewTile => {
                    if let Some(url) = self.read_clipboard() {
                        let url = Self::normalize_pasted_url(&url, &self.config);
                        if let Some(focused) = self.layout.focused() {
                            log::info!("PasteNewTile: from={:?} url={}", focused, url);
                            let new_id = self.views.create(&url);
                            self.layout.split(focused, hodei_core::types::SplitDirection::Vertical, new_id);
                            let (w, h) = self.tile_size(new_id);
                            if let Some(engine) = &mut self.engine {
                                engine.create_tile(new_id, &url, w, h);
                            }
                            self.resize_all_tiles();
                            self.update_hud();
                            self.request_redraw();
                        }
                    } else {
                        log::warn!("PasteNewTile: clipboard empty or unreadable");
                    }
                }
                Action::DuplicateTile => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(view) = self.views.get(focused) {
                            let url = view.url.clone();
                            log::info!("DuplicateTile: from={:?} url={}", focused, url);
                            let new_id = self.views.create(&url);
                            self.layout.split(focused, hodei_core::types::SplitDirection::Vertical, new_id);
                            let (w, h) = self.tile_size(new_id);
                            if let Some(engine) = &mut self.engine {
                                engine.create_tile(new_id, &url, w, h);
                            }
                            self.resize_all_tiles();
                            self.update_hud();
                            self.request_redraw();
                        }
                    }
                }
                Action::YankTitle => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(view) = self.views.get(focused) {
                            self.copy_to_clipboard(&view.title);
                            log::info!("Yanked title: {}", view.title);
                        }
                    }
                }
                Action::ViewSource => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(view) = self.views.get(focused) {
                            let url = format!("view-source:{}", view.url);
                            log::info!("ViewSource: view={:?} url={}", focused, url);
                            if let Some(engine) = &self.engine {
                                engine.navigate(focused, &url);
                            }
                        }
                    }
                }
                Action::GoHome => {
                    if let Some(focused) = self.layout.focused() {
                        let url = self.config.general.startup_url.clone();
                        log::info!("GoHome: view={:?} url={}", focused, url);
                        if let Some(engine) = &self.engine {
                            engine.navigate(focused, &url);
                        }
                        if let Some(view) = self.views.get_mut(focused) {
                            view.url = url;
                        }
                        self.update_hud();
                    }
                }
                Action::GoUp => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(view) = self.views.get(focused) {
                            if let Ok(mut url) = url::Url::parse(&view.url) {
                                let path = url.path().to_string();
                                let new_path = std::path::Path::new(&path)
                                    .parent()
                                    .and_then(|p| p.to_str())
                                    .unwrap_or("/");
                                let _ = url.set_path(new_path);
                                let url_str = url.to_string();
                                log::info!("GoUp: view={:?} -> {}", focused, url_str);
                                if let Some(engine) = &self.engine {
                                    engine.navigate(focused, &url_str);
                                }
                                self.views.get_mut(focused).unwrap().url = url_str;
                                self.update_hud();
                            }
                        }
                    }
                }
                Action::GoToRoot => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(view) = self.views.get(focused) {
                            if let Ok(mut url) = url::Url::parse(&view.url) {
                                let _ = url.set_path("/");
                                let url_str = url.to_string();
                                log::info!("GoToRoot: view={:?} -> {}", focused, url_str);
                                if let Some(engine) = &self.engine {
                                    engine.navigate(focused, &url_str);
                                }
                                self.views.get_mut(focused).unwrap().url = url_str;
                                self.update_hud();
                            }
                        }
                    }
                }
                Action::ResetSplits => {
                    log::info!("ResetSplits");
                    self.layout.reset_splits();
                    self.request_redraw();
                }
                Action::SwapTiles => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(last) = self.last_focused {
                            if last != focused {
                                log::info!("SwapTiles: {:?} <-> {:?}", focused, last);
                                self.layout.swap_tiles(focused, last);
                                self.last_focused = Some(focused);
                                self.request_redraw();
                            }
                        }
                    }
                }
                Action::SetQuickmark(slot) => {
                    if let Some(bookmarks) = &self.bookmarks {
                        if let Some(focused) = self.layout.focused() {
                            if let Some(view) = self.views.get(focused) {
                                let _ = bookmarks.set_quickmark(slot, &view.url, &view.title);
                                log::info!("Quickmark {} set: {}", slot, view.url);
                            }
                        }
                    }
                }
                Action::JumpQuickmark(slot) => {
                    if let Some(bookmarks) = &self.bookmarks {
                        if let Ok(Some(qm)) = bookmarks.get_quickmark(slot) {
                            if let Some(focused) = self.layout.focused() {
                                log::info!("JumpQuickmark: slot={} -> {}", slot, qm.url);
                                if let Some(engine) = &self.engine {
                                    engine.navigate(focused, &qm.url);
                                }
                                if let Some(view) = self.views.get_mut(focused) {
                                    view.url = qm.url;
                                }
                                self.update_hud();
                            }
                        }
                    }
                }
                Action::ToggleTheme => {
                    self.theme_dark = !self.theme_dark;
                    log::info!("Theme toggled: {}", if self.theme_dark { "dark" } else { "light" });
                    if let Some(engine) = &self.engine {
                        engine.set_theme(self.theme_dark);
                    }
                }
                Action::ShowShortcuts => {
                    self.show_shortcuts = !self.show_shortcuts;
                    log::info!("ShowShortcuts: {}", self.show_shortcuts);
                    self.update_hud();
                }
                Action::DevToolsShow => {
                    if let Some((port, token)) = &self.devtools_status {
                        let ws_url = self.devtools_bridge.as_ref()
                            .map(|b| b.ws_url())
                            .unwrap_or_else(|| format!("ws://127.0.0.1:{}/", self.config.devtools.ws_port));
                        log::info!(
                            "DevTools: TCP=127.0.0.1:{} WS={} token={}",
                            port, ws_url, token
                        );
                        if let Some(focused) = self.layout.focused() {
                            self.status_texts.insert(
                                focused,
                                format!("DevTools TCP:{} WS:{} token:{}", port, ws_url, token),
                            );
                        }
                    } else {
                        log::info!("DevTools server not yet started");
                        if let Some(focused) = self.layout.focused() {
                            self.status_texts.insert(focused, "DevTools: starting...".to_string());
                        }
                    }
                    self.update_hud();
                }
            }
        }
    }

    /// Normalize a pasted string into a navigable URL.
    fn normalize_pasted_url(text: &str, config: &Config) -> String {
        let trimmed = text.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else {
            config.search_url(trimmed)
        }
    }

    fn update_suggestions(&mut self) {
        if let Mode::Command { buffer } = self.input.mode() {
            let trimmed = buffer.trim();
            if let Some(query) = trimmed.strip_prefix("open ").or_else(|| trimmed.strip_prefix("o ")) {
                if query.is_empty() {
                    self.suggestions.clear();
                    self.suggestion_index = 0;
                } else {
                    let mut all = Vec::new();

                    if let Some(bm) = &self.bookmarks {
                        if let Ok(bookmarks) = bm.search(query, 10) {
                            for b in bookmarks {
                                let url_score = suggest::score(query, &b.url);
                                let title_score = suggest::score(query, &b.title);
                                all.push(Suggestion {
                                    url: b.url,
                                    title: b.title,
                                    source: SuggestionSource::Bookmark,
                                    score: url_score.max(title_score),
                                });
                            }
                        }
                    }

                    if let Some(hm) = &self.history {
                        if let Ok(entries) = hm.search(query, 10) {
                            for h in entries {
                                if all.iter().any(|s| s.url == h.url) {
                                    continue;
                                }
                                let url_score = suggest::score(query, &h.url);
                                let title_score = suggest::score(query, &h.title);
                                all.push(Suggestion {
                                    url: h.url,
                                    title: h.title,
                                    source: SuggestionSource::History,
                                    score: url_score.max(title_score),
                                });
                            }
                        }
                    }

                    self.suggestions = suggest::rank_suggestions(all, 10);
                    self.suggestion_index = 0;
                    log::debug!("update_suggestions: query='{}' -> {} suggestions", query, self.suggestions.len());
                }
            } else {
                self.suggestions.clear();
                self.suggestion_index = 0;
            }
        } else {
            self.suggestions.clear();
            self.suggestion_index = 0;
        }

        self.update_suggestion_display();
    }

    fn update_suggestion_display(&self) {
        if let Some(hud) = &self.hud {
            if self.suggestions.is_empty() {
                hud.set_suggestions_visible(false);
            } else {
                let items: Vec<(String, String, bool)> = self.suggestions
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (s.title.clone(), s.url.clone(), i == self.suggestion_index))
                    .collect();
                hud.set_suggestions(items);
                hud.set_suggestions_visible(true);
            }
            hud.request_redraw();
        }
    }

    fn update_hud(&self) {
        log::trace!("update_hud");
        if let Some(hud) = &self.hud {
            let mode_str = match self.input.mode() {
                Mode::Normal => "NORMAL",
                Mode::Insert => "INSERT",
                Mode::Command { .. } => "COMMAND",
                Mode::Hint { .. } => "HINT",
                Mode::Search { .. } => "SEARCH",
            };
            hud.set_mode_text(mode_str);

            if let Mode::Command { buffer } = self.input.mode() {
                hud.set_command_visible(true);
                hud.set_command_text(buffer);
            } else {
                hud.set_command_visible(false);
            }

            let tile_count = self.layout.resolve().len();
            hud.set_tile_count(tile_count as i32);

            let focused = self.layout.focused();
            if let Some(focused_id) = focused {
                if let Some(view) = self.views.get(focused_id) {
                    hud.set_url_text(&view.url);
                    hud.set_title_text(&view.title);

                    let lower = view.url.to_ascii_lowercase();
                    let secure = lower.starts_with("https://");
                    let insecure = lower.starts_with("http://");
                    hud.set_secure(secure);
                    hud.set_insecure(insecure);

                    let bookmarked = self
                        .bookmarks
                        .as_ref()
                        .and_then(|b| b.is_bookmarked(&view.url).ok())
                        .unwrap_or(false);
                    hud.set_bookmarked(bookmarked);
                }
                let status = self.status_texts.get(&focused_id).cloned().unwrap_or_default();
                hud.set_status_text(&status);

                let zoom = self.tile_zoom_levels.get(&focused_id).copied().unwrap_or(1.0);
                hud.set_zoom(zoom);
            }

            if let Some(focused_id) = focused {
                let loading = matches!(
                    self.tile_load_status.get(&focused_id),
                    Some(TileLoadStatus::Started) | Some(TileLoadStatus::HeadParsed)
                );
                let (can_back, can_forward) = self
                    .tile_nav_availability
                    .get(&focused_id)
                    .copied()
                    .unwrap_or((false, false));
                hud.set_loading(loading);
                hud.set_can_back(can_back);
                hud.set_can_forward(can_forward);
            } else {
                hud.set_loading(false);
                hud.set_can_back(false);
                hud.set_can_forward(false);
            }

            hud.set_shortcuts_visible(self.show_shortcuts);

            hud.request_redraw();
        }
    }

    fn update_hint_display(&self) {
        if let Some(hud) = &self.hud {
            if let Mode::Hint { filter, labels } = self.input.mode() {
                let display: Vec<(String, f32, f32, bool)> = labels
                    .iter()
                    .enumerate()
                    .filter_map(|(i, label)| {
                        self.hint_elements.get(i).map(|elem| {
                            let active = label.starts_with(filter.as_str());
                            (label.clone(), elem.x as f32, elem.y as f32, active)
                        })
                    })
                    .collect();
                log::trace!("update_hint_display: filter='{}' displaying {} hints", filter, display.len());
                hud.set_hints(display);
                hud.request_redraw();
            }
        }
    }

    fn process_metadata_events(&mut self) {
        let events = match &self.engine {
            Some(engine) => engine.drain_metadata_events(),
            None => return,
        };
        let mut needs_hud_update = false;
        for event in events {
            log::debug!("process_metadata_events: {:?}", event);
            match event {
                MetadataEvent::UrlChanged { view_id, url } => {
                    if let Some(view) = self.views.get_mut(view_id) {
                        view.url = url.clone();
                        needs_hud_update = true;
                    }
                    if let Some(history) = &self.history {
                        let title = self.views.get(view_id).map(|v| v.title.as_str()).unwrap_or("");
                        let _ = history.record_visit(&url, title);
                    }
                }
                MetadataEvent::TitleChanged { view_id, title } => {
                    if let Some(view) = self.views.get_mut(view_id) {
                        view.title = title;
                        needs_hud_update = true;
                    }
                }
                MetadataEvent::StatusTextChanged { view_id, text } => {
                    if let Some(t) = text {
                        if !t.is_empty() {
                            self.status_texts.insert(view_id, t);
                        } else {
                            self.status_texts.remove(&view_id);
                        }
                    } else {
                        self.status_texts.remove(&view_id);
                    }
                    needs_hud_update = true;
                }
                MetadataEvent::FrameReady { .. } => {
                    self.pending_paint = true;
                    self.request_redraw();
                }
                MetadataEvent::LoadStatusChanged { view_id, status } => {
                    self.tile_load_status.insert(view_id, status);
                    needs_hud_update = true;
                }
                MetadataEvent::HistoryChanged { view_id, can_back, can_forward } => {
                    self.tile_nav_availability.insert(view_id, (can_back, can_forward));
                    needs_hud_update = true;
                }
                MetadataEvent::DevToolsStarted { port, token } => {
                    log::info!("DevTools TCP server on port {} (token: {})", port, token);
                    self.devtools_status = Some((port, token.clone()));
                    let mut bridge = crate::devtools_bridge::DevToolsBridge::new(
                        port,
                        self.config.devtools.ws_port,
                    );
                    bridge.spawn();
                    self.devtools_bridge = Some(bridge);
                }
                MetadataEvent::DevToolsConnectionRequest => {
                    // Auto-allowed by delegate; no HUD action needed
                }
            }
        }
        if needs_hud_update {
            self.update_hud();
        }
    }

    fn process_hint_results(&mut self) {
        while let Ok(json) = self.hint_result_rx.try_recv() {
            match hint::parse_hint_elements(&json) {
                Ok(elements) => {
                    let labels = hint::generate_labels(elements.len());
                    log::info!("process_hint_results: parsed {} hint elements", elements.len());
                    self.hint_labels = labels.clone();
                    self.hint_elements = elements;
                    self.input.enter_hint_mode(labels);
                    self.update_hud();
                    self.update_hint_display();
                }
                Err(e) => {
                    log::warn!("Failed to parse hint elements: {}", e);
                }
            }
        }
    }

    fn inject_search_init(&self, query: &str) {
        if let Some(focused) = self.layout.focused() {
            if let Some(engine) = &self.engine {
                let tx = self.search_result_tx.clone();
                let q = query.to_string();
                log::trace!("inject_search_init: view={:?} query='{}'", focused, q);
                engine.evaluate_js(
                    focused,
                    &format!("{}('{}')", search::SEARCH_INIT_SCRIPT.trim_end_matches("()"), q.replace('\\', "\\\\").replace('\'', "\\'")),
                    Box::new(move |result| {
                        if let Ok(json) = result {
                            if let Ok(count) = serde_json::from_str::<serde_json::Value>(&json)
                                .and_then(|v| Ok(v.get("count").and_then(|c| c.as_u64()).unwrap_or(0) as usize))
                            {
                                let _ = tx.send(search::SearchResult { index: if count > 0 { 1 } else { 0 }, count });
                            }
                        }
                    }),
                );
            }
        }
    }

    fn inject_search_navigate(&self, offset: i32) {
        if let Some(focused) = self.layout.focused() {
            if let Some(engine) = &self.engine {
                let tx = self.search_result_tx.clone();
                log::trace!("inject_search_navigate: view={:?} offset={}", focused, offset);
                engine.evaluate_js(
                    focused,
                    &format!("{}({})", search::SEARCH_NAVIGATE_SCRIPT.trim_end_matches("()"), offset),
                    Box::new(move |result| {
                        if let Ok(json) = result {
                            if let Ok((index, count)) = serde_json::from_str::<serde_json::Value>(&json)
                                .and_then(|v| {
                                    let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                    let count = v.get("count").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
                                    Ok((index, count))
                                })
                            {
                                let _ = tx.send(search::SearchResult { index, count });
                            }
                        }
                    }),
                );
            }
        }
    }

    fn inject_search_clear(&self) {
        if let Some(focused) = self.layout.focused() {
            if let Some(engine) = &self.engine {
                log::trace!("inject_search_clear: view={:?}", focused);
                engine.evaluate_js(
                    focused,
                    &format!("{}()", search::SEARCH_CLEAR_SCRIPT),
                    Box::new(|_| {}),
                );
            }
        }
    }

    fn process_search_results(&mut self) {
        while let Ok(result) = self.search_result_rx.try_recv() {
            log::debug!("process_search_results: {:?}", result);
            self.current_search_result = result.clone();
            if let Some(hud) = &self.hud {
                hud.set_search_info(&result.info_string());
                hud.request_redraw();
            }
        }
    }

    fn adjust_zoom(&mut self, delta: f32) {
        if let Some(focused) = self.layout.focused() {
            let current = self.tile_zoom_levels.get(&focused).copied().unwrap_or(1.0);
            let new_zoom = (current + delta).clamp(0.25, 5.0);
            log::info!("adjust_zoom: view={:?} {} -> {}", focused, current, new_zoom);
            self.tile_zoom_levels.insert(focused, new_zoom);
            self.apply_zoom(focused, new_zoom);
        }
    }

    fn apply_zoom(&self, view_id: ViewId, zoom: f32) {
        if let Some(engine) = &self.engine {
            engine.set_page_zoom(view_id, zoom);
        }
    }

    fn inject_scroll(&self, view_id: ViewId, script: &str) {
        if let Some(engine) = &self.engine {
            log::trace!("inject_scroll: view={:?} script_len={}", view_id, script.len());
            engine.evaluate_js(view_id, script, Box::new(|_| {}));
        }
    }

    fn copy_to_clipboard(&self, text: &str) {
        use std::process::{Command, Stdio};
        #[cfg(target_os = "macos")]
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .expect("pbcopy failed");
        #[cfg(target_os = "linux")]
        let mut child = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .or_else(|_| Command::new("xclip").args(["-selection", "clipboard"]).stdin(Stdio::piped()).spawn())
            .expect("clipboard copy failed");
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        compile_error!("clipboard not implemented for this OS");
        
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        log::debug!("copy_to_clipboard: wrote {} bytes to clipboard", text.len());
    }

    fn read_clipboard(&self) -> Option<String> {
        use std::process::{Command, Stdio};
        #[cfg(target_os = "macos")]
        let output = Command::new("pbpaste")
            .stdout(Stdio::piped())
            .output()
            .ok()?;
        #[cfg(target_os = "linux")]
        let output = Command::new("wl-paste")
            .stdout(Stdio::piped())
            .output()
            .or_else(|_| Command::new("xclip").args(["-selection", "clipboard", "-o"]).stdout(Stdio::piped()).output())
            .ok()?;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        compile_error!("clipboard not implemented for this OS");

        let result = String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        log::debug!("read_clipboard: result={:?}", result.as_ref().map(|s| s.len()));
        result
    }

    /// Is the given window-local physical-pixel point inside a HUD region
    /// that should consume the pointer event (top icon bar, bottom status bar,
    /// command/search overlay, or the shortcuts modal)?
    fn is_hud_point(&self, _x: f32, y: f32) -> bool {
        let Some(hud) = &self.hud else { return false };
        let sf = hud.scale_factor();
        let h = hud.height() as f32;
        // Shortcuts modal is full-screen when visible.
        if self.show_shortcuts {
            return true;
        }
        // Top icon bar (NORMAL mode only) — 34 logical px tall.
        let in_normal = matches!(self.input.mode(), Mode::Normal);
        if in_normal && y < 34.0 * sf {
            return true;
        }
        // Bottom status bar — 26 logical px tall.
        if y > h - 26.0 * sf {
            return true;
        }
        // Command / search bar sits just above the status bar (28 logical px).
        // We only swallow clicks there if it's visible; otherwise the bar is
        // invisible and the click should fall through to Servo.
        false
    }

    fn click_to_focus(&mut self, x: f32, y: f32) {
        let resolved = self.layout.resolve();
        for (view_id, rect) in resolved {
            if rect.contains(x, y) {
                if self.layout.focused() != Some(view_id) {
                    log::info!("click_to_focus: ({}, {}) -> view {:?}", x, y, view_id);
                    self.layout.set_focused(view_id);
                    self.update_hud();
                    self.request_redraw();
                }
                break;
            }
        }
    }

    fn dispatch_mouse_event(&self, event: CoreMouseEvent) {
        // Only forward mouse events when in Insert mode or for clicks/scrolling
        if let Some(focused) = self.layout.focused() {
            // Convert global window coordinates to tile-local coordinates
            let local_event = if let Some((_, rect)) = self.layout.resolve().iter().find(|(id, _)| *id == focused) {
                match event {
                    CoreMouseEvent::Move { x, y } => CoreMouseEvent::Move {
                        x: x - rect.x,
                        y: y - rect.y,
                    },
                    CoreMouseEvent::Down { x, y, button } => CoreMouseEvent::Down {
                        x: x - rect.x,
                        y: y - rect.y,
                        button,
                    },
                    CoreMouseEvent::Up { x, y, button } => CoreMouseEvent::Up {
                        x: x - rect.x,
                        y: y - rect.y,
                        button,
                    },
                    CoreMouseEvent::Scroll { x, y, delta_x, delta_y } => CoreMouseEvent::Scroll {
                        x: x - rect.x,
                        y: y - rect.y,
                        delta_x,
                        delta_y,
                    },
                }
            } else {
                event
            };
            log::trace!("dispatch_mouse_event: view={:?} local_event={:?}", focused, local_event);
            if let Some(engine) = &self.engine {
                engine.send_mouse_event(focused, local_event);
            }
        }
    }

    fn tile_size(&self, view_id: ViewId) -> (u32, u32) {
        self.layout
            .resolve()
            .into_iter()
            .find(|(id, _)| *id == view_id)
            .map(|(_, r)| (r.width.max(1.0) as u32, r.height.max(1.0) as u32))
            .unwrap_or_else(|| {
                let vp = self.layout_viewport();
                (vp.width.max(1.0) as u32, vp.height.max(1.0) as u32)
            })
    }

    fn resize_all_tiles(&mut self) {
        let resolved = self.layout.resolve();
        log::debug!("resize_all_tiles: resizing {} tiles", resolved.len());
        if let Some(engine) = &mut self.engine {
            for (view_id, rect) in &resolved {
                engine.resize_tile(*view_id, rect.width as u32, rect.height as u32);
            }
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            log::trace!("request_redraw");
            window.request_redraw();
        }
    }

    fn layout_viewport(&self) -> Rect {
        self.window
            .as_ref()
            .map(|w| {
                let size = w.inner_size();
                Rect::new(0.0, 0.0, size.width as f32, size.height as f32)
            })
            .unwrap_or_default()
    }

    fn collect_tile_rows(&self) -> Vec<TileRow> {
        self.views.all_views().iter().filter_map(|id| {
            self.views.get(*id).map(|v| TileRow {
                view_id: v.id,
                url: v.url.clone(),
                title: v.title.clone(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            })
        }).collect()
    }

    fn autosave(&mut self) {
        let (nodes, focused) = self.layout.serialize();
        let tiles = self.collect_tile_rows();
        if let Some(workspace) = &mut self.workspace {
            if let Err(e) = workspace.save_active(&nodes, &tiles, focused) {
                log::error!("Autosave failed: {}", e);
            } else {
                log::info!("Autosave successful");
            }
        }
    }

    fn convert_key_event(&self, event: &winit::event::KeyEvent) -> Option<CoreKeyEvent> {
        use winit::keyboard::{Key, NamedKey};
        let state = match event.state {
            winit::event::ElementState::Pressed => KeyState::Pressed,
            winit::event::ElementState::Released => KeyState::Released,
        };
        let key = match &event.logical_key {
            Key::Named(NamedKey::Escape) => CoreKey::Escape,
            Key::Named(NamedKey::Enter) => CoreKey::Enter,
            Key::Named(NamedKey::Backspace) => CoreKey::Backspace,
            Key::Named(NamedKey::Tab) => CoreKey::Tab,
            Key::Named(NamedKey::ArrowLeft) => CoreKey::Left,
            Key::Named(NamedKey::ArrowRight) => CoreKey::Right,
            Key::Named(NamedKey::ArrowUp) => CoreKey::Up,
            Key::Named(NamedKey::ArrowDown) => CoreKey::Down,
            Key::Named(NamedKey::Home) => CoreKey::Home,
            Key::Named(NamedKey::End) => CoreKey::End,
            Key::Named(NamedKey::PageUp) => CoreKey::PageUp,
            Key::Named(NamedKey::PageDown) => CoreKey::PageDown,
            Key::Character(c) => {
                let ch = c.chars().next()?;
                CoreKey::Char(ch)
            }
            _ => return None,
        };
        let modifiers = Modifiers {
            ctrl: self.modifiers.control_key(),
            shift: self.modifiers.shift_key(),
            alt: self.modifiers.alt_key(),
            meta: self.modifiers.super_key(),
        };
        let core_event = CoreKeyEvent { key, state, modifiers };
        log::trace!("convert_key_event: winit={:?} -> core={:?}", event.logical_key, core_event.key);
        Some(core_event)
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        log::info!("App::resumed: creating window");
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("Hodei")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 800u32)),
            )
            .expect("Failed to create window");

        let size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        log::info!(
            "App::resumed: window created {}x{} scale_factor={}",
            size.width, size.height, scale_factor
        );

        // Initialize Servo engine
        use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
        let display_handle = window.display_handle().unwrap();
        let window_handle = window.window_handle().unwrap();
        let waker = Box::new(WinitWaker { proxy: self.proxy.clone() });

        let preferences = if self.config.devtools.enabled {
            log::info!("DevTools enabled on TCP port {}", self.config.devtools.tcp_port);
            Some(Engine::devtools_preferences(self.config.devtools.tcp_port))
        } else {
            None
        };
        let mut engine = Engine::new(display_handle, window_handle, (size.width, size.height), waker, preferences);

        // Initialize HUD
        let hud = Hud::new(size.width, size.height, scale_factor);

        // Wire HUD click callbacks to dispatch Actions back to the main loop.
        // Each callback fires on the main thread (Slint is single-threaded via
        // our custom Platform), but we can't borrow `self` inside an `Fn`, so
        // we hop through the winit EventLoopProxy.
        {
            use hodei_core::input::Action;
            use hodei_core::types::SplitDirection;
            let px = |a: Action| {
                let proxy = self.proxy.clone();
                move || { let _ = proxy.send_event(UserEvent::HudAction(a.clone())); }
            };
            hud.on_clicked_back(px(Action::Back));
            hud.on_clicked_forward(px(Action::Forward));
            hud.on_clicked_reload(px(Action::Reload));
            hud.on_clicked_home(px(Action::GoHome));
            hud.on_clicked_split_v(px(Action::SplitView(SplitDirection::Vertical)));
            hud.on_clicked_split_h(px(Action::SplitView(SplitDirection::Horizontal)));
            hud.on_clicked_close(px(Action::CloseView));
            hud.on_clicked_swap(px(Action::SwapTiles));
            hud.on_clicked_hint(px(Action::EnterHintMode));
            hud.on_clicked_search(px(Action::EnterSearch));
            hud.on_clicked_command(px(Action::EnterCommand));
            hud.on_clicked_bookmark(px(Action::Bookmark(None)));
            hud.on_clicked_shortcuts_dismiss(px(Action::ShowShortcuts));
        }

        // Initialize layout
        self.layout = BspLayout::new(Rect::new(0.0, 0.0, size.width as f32, size.height as f32));

        // Open database and initialize managers
        let db_path = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("hodei")
            .join("hodei.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let mut restored = false;
        if let Ok(conn) = db::open_database(&db_path) {
            log::info!("App::resumed: database opened at {:?}", db_path);
            self.history = Some(HistoryManager::new(conn.clone(), self.config.history.max_entries));
            self.bookmarks = Some(BookmarkManager::new(conn.clone()));
            let mut wm = WorkspaceManager::new(conn);

            // Try to restore last workspace
            if self.config.general.restore_workspace_on_startup {
                log::info!("App::resumed: attempting to restore default workspace");
                if let Ok(Some(state)) = wm.switch_to("default", &[], &[], None) {
                    if !state.tiles.is_empty() {
                        log::info!("App::resumed: restoring {} tiles", state.tiles.len());
                        self.layout = BspLayout::deserialize(
                            Rect::new(0.0, 0.0, size.width as f32, size.height as f32),
                            &state.nodes,
                            state.focused,
                        );
                        let resolved = self.layout.resolve();
                        let size_for = |id: ViewId| -> (u32, u32) {
                            resolved.iter().find(|(i, _)| *i == id)
                                .map(|(_, r)| (r.width.max(1.0) as u32, r.height.max(1.0) as u32))
                                .unwrap_or((size.width, size.height))
                        };
                        for tile in &state.tiles {
                            self.views.create_with_id(tile.view_id, &tile.url);
                            let (w, h) = size_for(tile.view_id);
                            engine.create_tile(tile.view_id, &tile.url, w, h);
                        }
                        restored = true;
                    } else {
                        log::info!("App::resumed: default workspace is empty");
                    }
                }
                wm.set_active("default");
            }

            self.workspace = Some(wm);
        } else {
            log::warn!("App::resumed: failed to open database at {:?}", db_path);
        }

        if !restored {
            let startup_url = self.config.general.startup_url.clone();
            log::info!("App::resumed: creating initial tile with {}", startup_url);
            let view_id = self.views.create(&startup_url);
            self.layout.add_first_view(view_id);
            engine.create_tile(view_id, &startup_url, size.width, size.height);
        }

        // Resize all tiles to match the initial layout rects.
        let resolved = self.layout.resolve();
        for (view_id, rect) in &resolved {
            engine.resize_tile(*view_id, rect.width as u32, rect.height as u32);
        }

        // Tell Servo about the display scale so page content is rendered crisply
        // on HiDPI displays rather than filling only a fraction of the FBO.
        engine.set_hidpi_scale_factor(scale_factor);

        // Ensure window context is current before creating compositor resources (VAO, shaders).
        // create_tile() above may have switched the active GL context to an offscreen one.
        engine.prepare_window_for_rendering();

        // Initialize compositor
        let gl = engine.gl_context();
        let compositor = unsafe { Compositor::new(&gl, size.width, size.height) };

        self.window = Some(window);
        self.engine = Some(engine);
        self.hud = Some(hud);
        self.compositor = Some(compositor);
        self.update_hud();
        self.request_redraw();
        log::info!("App::resumed: fully initialized");
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                log::info!("WindowEvent::CloseRequested");
                self.autosave();
                if let Some(mut bridge) = self.devtools_bridge.take() {
                    bridge.stop();
                }
                event_loop.exit();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(core_event) = self.convert_key_event(&event) {
                    if self.show_shortcuts {
                        if core_event.state == KeyState::Pressed
                            && (core_event.key == CoreKey::Escape
                                || core_event.key == CoreKey::Char('?'))
                        {
                            log::debug!("KeyboardInput: closing shortcuts modal");
                            self.show_shortcuts = false;
                            self.update_hud();
                        }
                        return;
                    }
                    let actions = self.input.handle(&core_event);
                    if !actions.is_empty() {
                        log::debug!("KeyboardInput: {:?} -> {} action(s)", core_event.key, actions.len());
                    }
                    self.dispatch_actions(actions);
                } else {
                    log::trace!("KeyboardInput: unmapped key {:?}", event.logical_key);
                }
            }

            WindowEvent::ModifiersChanged(new_modifiers) => {
                log::trace!("ModifiersChanged: {:?}", new_modifiers.state());
                self.modifiers = new_modifiers.state();
            }

            WindowEvent::CursorMoved { position, .. } => {
                log::trace!("CursorMoved: ({}, {})", position.x, position.y);
                self.mouse_position = (position.x, position.y);
                // Always let the HUD track pointer moves so hover states work.
                if let Some(hud) = &self.hud {
                    hud.dispatch_pointer_moved(position.x as f32, position.y as f32);
                }
                if self.is_hud_point(position.x as f32, position.y as f32) {
                    // Hover in the HUD bar — don't also scroll/drag the page.
                    self.request_redraw();
                    return;
                }
                self.dispatch_mouse_event(CoreMouseEvent::Move {
                    x: position.x as f32,
                    y: position.y as f32,
                });
            }

            WindowEvent::CursorLeft { .. } => {
                if let Some(hud) = &self.hud {
                    hud.dispatch_pointer_exited();
                }
                self.request_redraw();
            }

            WindowEvent::MouseInput { state: button_state, button, .. } => {
                let (mx, my) = self.mouse_position;
                // Handle mouse back/forward buttons for navigation
                if button_state == winit::event::ElementState::Pressed {
                    match button {
                        winit::event::MouseButton::Back => {
                            log::debug!("MouseInput: Back button pressed");
                            self.dispatch_actions(vec![Action::Back]);
                            return;
                        }
                        winit::event::MouseButton::Forward => {
                            log::debug!("MouseInput: Forward button pressed");
                            self.dispatch_actions(vec![Action::Forward]);
                            return;
                        }
                        _ => {}
                    }
                }
                let core_button = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    winit::event::MouseButton::Back => MouseButton::Left,
                    winit::event::MouseButton::Forward => MouseButton::Left,
                    winit::event::MouseButton::Other(_) => MouseButton::Left,
                };
                let event = match button_state {
                    winit::event::ElementState::Pressed => CoreMouseEvent::Down {
                        x: mx as f32,
                        y: my as f32,
                        button: core_button,
                    },
                    winit::event::ElementState::Released => CoreMouseEvent::Up {
                        x: mx as f32,
                        y: my as f32,
                        button: core_button,
                    },
                };
                // Route left-button clicks to the HUD first if they land in a
                // HUD region. Slint's TouchAreas fire their callbacks via the
                // EventLoopProxy, and we swallow the event so it doesn't
                // propagate into the webview underneath.
                if matches!(button, winit::event::MouseButton::Left)
                    && self.is_hud_point(mx as f32, my as f32)
                {
                    if let Some(hud) = &self.hud {
                        match button_state {
                            winit::event::ElementState::Pressed => {
                                hud.dispatch_pointer_pressed(mx as f32, my as f32);
                            }
                            winit::event::ElementState::Released => {
                                hud.dispatch_pointer_released(mx as f32, my as f32);
                            }
                        }
                    }
                    self.request_redraw();
                    return;
                }
                // Click-to-focus: if clicking inside a non-focused tile, focus it
                if let CoreMouseEvent::Down { x, y, .. } = event {
                    self.click_to_focus(x, y);
                }
                log::trace!("MouseInput: {:?} {:?} at ({}, {})", button_state, button, mx, my);
                self.dispatch_mouse_event(event);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (mx, my) = self.mouse_position;
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                log::trace!("MouseWheel: delta={:?} -> dx={} dy={}", delta, dx, dy);
                self.dispatch_mouse_event(CoreMouseEvent::Scroll {
                    x: mx as f32,
                    y: my as f32,
                    delta_x: dx,
                    delta_y: dy,
                });
            }

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let sf = scale_factor as f32;
                log::info!("WindowEvent::ScaleFactorChanged: {}", sf);
                if let Some(engine) = &self.engine {
                    engine.set_hidpi_scale_factor(sf);
                }
                if let Some(hud) = &mut self.hud {
                    hud.set_scale_factor(sf);
                }
                self.request_redraw();
            }

            WindowEvent::Resized(size) => {
                log::info!("WindowEvent::Resized: {}x{}", size.width, size.height);
                self.layout.set_viewport(Rect::new(
                    0.0, 0.0,
                    size.width as f32, size.height as f32,
                ));
                // Resize the window rendering context and all tiles.
                if let Some(engine) = &mut self.engine {
                    engine.resize_window(size.width, size.height);
                }
                self.resize_all_tiles();
                if let Some(hud) = &mut self.hud {
                    hud.resize(size.width, size.height);
                }
                if let (Some(engine), Some(compositor)) = (&self.engine, &mut self.compositor) {
                    let gl = engine.gl_context();
                    unsafe { compositor.resize(&gl, size.width, size.height); }
                }
                // Nudge Servo so it can lay out the new viewport and produce a
                // fresh frame before we blit. Without this, the next RedrawRequested
                // would blit the stale (pre-resize) offscreen FBO and you'd see
                // the page content stretched/cropped until Servo caught up a
                // frame later.
                if let Some(engine) = &mut self.engine {
                    engine.spin();
                }
                self.pending_paint = true;
                self.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                let Some(engine) = &self.engine else { return };
                let resolved = self.layout.resolve();
                let win_size = self
                    .window
                    .as_ref()
                    .map(|w| w.inner_size())
                    .unwrap_or(winit::dpi::PhysicalSize::new(0, 0));
                let window_h = win_size.height as i32;
                log::trace!("RedrawRequested: {} tiles, window={}x{}", resolved.len(), win_size.width, win_size.height);

                if self.pending_paint {
                    log::trace!("RedrawRequested: pending_paint=true, painting {} tiles", resolved.len());
                    self.pending_paint = false;
                    for (view_id, _) in &resolved {
                        engine.paint_tile(*view_id);
                    }
                }

                engine.prepare_window_for_rendering();
                let gl = engine.gl_context();
                unsafe {
                    gl.viewport(0, 0, win_size.width as i32, window_h);
                    gl.disable(glow::SCISSOR_TEST);
                    gl.clear_color(0.05, 0.05, 0.1, 1.0);
                    log::trace!("RedrawRequested: clearing framebuffer");
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }

                for (view_id, rect) in &resolved {
                    // Layout rects use top-left origin; GL wants bottom-left.
                    let gl_y = window_h - rect.y as i32 - rect.height as i32;
                    let target = euclid::default::Rect::new(
                        euclid::default::Point2D::new(rect.x as i32, gl_y),
                        euclid::default::Size2D::new(rect.width as i32, rect.height as i32),
                    );
                    log::trace!("RedrawRequested: blitting view {:?} to {:?}", view_id, target);
                    engine.blit_tile_to_window(*view_id, target);
                }

                if let (Some(hud), Some(compositor)) = (self.hud.as_mut(), self.compositor.as_ref()) {
                    let hud_buffer = hud.render();
                    log::trace!("RedrawRequested: compositing HUD ({} bytes)", hud_buffer.len());
                    unsafe { compositor.draw_hud(&gl, hud_buffer); }
                }

                engine.present();
                log::trace!("RedrawRequested: present complete");

                // Debug: dump the composited framebuffer to a PNG for vision-based
                // verification. Gated on HODEI_SCREENSHOT=1; throttled to 1Hz to
                // stay out of the critical path.
                if std::env::var_os("HODEI_SCREENSHOT").as_deref() == Some(std::ffi::OsStr::new("1")) {
                    drop(gl);
                    let engine = self.engine.as_ref().unwrap();
                    let gl2 = engine.gl_context();
                    self.maybe_dump_screenshot(&gl2, win_size.width, win_size.height);
                }
            }

            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::ServoTick => {
                log::trace!("UserEvent::ServoTick");
                if let Some(engine) = &mut self.engine {
                    engine.spin();
                }
                self.process_metadata_events();
                self.process_hint_results();
                self.process_search_results();
                self.request_redraw();
            }
            UserEvent::HudAction(action) => {
                log::info!("UserEvent::HudAction: {:?}", action);
                self.dispatch_actions(vec![action]);
                self.request_redraw();
            }
        }
    }
}
