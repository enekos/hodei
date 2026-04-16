use std::sync::mpsc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowId};
use glow::HasContext;

use orthogonal_core::bookmarks::BookmarkManager;
use orthogonal_core::compositor::Compositor;
use orthogonal_core::config::Config;
use orthogonal_core::db;
use orthogonal_core::hint;
use orthogonal_core::history::HistoryManager;
use orthogonal_core::search;
use orthogonal_core::hud::Hud;
use orthogonal_core::input::{Action, InputRouter, Mode};
use orthogonal_core::layout::BspLayout;
use orthogonal_core::suggest::{self, Suggestion, SuggestionSource};
use orthogonal_core::types::*;
use orthogonal_core::view::ViewManager;
use orthogonal_core::workspace::WorkspaceManager;
use orthogonal_servo::Engine;

use crate::UserEvent;

/// Wraps EventLoopProxy as a Servo EventLoopWaker.
struct WinitWaker {
    proxy: EventLoopProxy<UserEvent>,
}

impl orthogonal_servo::ServoEventLoopWaker for WinitWaker {
    fn clone_box(&self) -> Box<dyn orthogonal_servo::ServoEventLoopWaker> {
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
    tile_textures: std::collections::HashMap<ViewId, glow::Texture>,
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
    tile_zoom_levels: std::collections::HashMap<ViewId, f32>,
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let config_path = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("orthogonal")
            .join("config.toml");
        let config = Config::load(&config_path);
        let input = InputRouter::with_overrides(&config.keybindings);
        let (hint_result_tx, hint_result_rx) = mpsc::channel();
        let (search_result_tx, search_result_rx) = mpsc::channel();

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
            tile_textures: std::collections::HashMap::new(),
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
            tile_zoom_levels: std::collections::HashMap::new(),
        }
    }

    fn dispatch_actions(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::FocusNeighbor(dir) => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(neighbor) = self.layout.focus_neighbor(focused, dir) {
                            self.layout.set_focused(neighbor);
                            self.update_hud();
                        }
                    }
                }
                Action::SplitView(dir) => {
                    if let Some(focused) = self.layout.focused() {
                        let new_id = self.views.create("about:blank");
                        self.layout.split(focused, dir, new_id);
                        if let Some(engine) = &mut self.engine {
                            engine.create_tile(new_id, "about:blank");
                        }
                        self.update_hud();
                        self.request_redraw();
                    }
                }
                Action::CloseView => {
                    if let Some(focused) = self.layout.focused() {
                        self.layout.close(focused);
                        self.views.remove(focused);
                        self.tile_textures.remove(&focused);
                        if let Some(engine) = &mut self.engine {
                            engine.destroy_tile(focused);
                        }
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
                        self.layout.resize_split(focused, signed_delta);
                        self.request_redraw();
                    }
                }
                Action::ForwardToServo(key_event) => {
                    if let Some(focused) = self.layout.focused() {
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
                        if let Some(engine) = &self.engine {
                            engine.go_back(focused);
                        }
                    }
                }
                Action::Forward => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(engine) = &self.engine {
                            engine.go_forward(focused);
                        }
                    }
                }
                Action::Reload => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(view) = self.views.get(focused) {
                            let url = view.url.clone();
                            if let Some(engine) = &self.engine {
                                engine.navigate(focused, &url);
                            }
                        }
                    }
                }
                Action::EnterHintMode => {
                    if let Some(focused) = self.layout.focused() {
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
                Action::HintCharTyped(_) => {
                    self.update_hint_display();
                }
                Action::EnterInsert | Action::EnterCommand | Action::ExitToNormal => {
                    self.update_hud();
                }
                Action::EnterSearch => {
                    if let Some(hud) = &self.hud {
                        hud.set_search_visible(true);
                        hud.set_search_text("");
                        hud.request_redraw();
                    }
                    self.update_hud();
                }
                Action::SearchQueryChanged(query) => {
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
                    self.inject_search_navigate(1);
                }
                Action::SearchPrev => {
                    self.inject_search_navigate(-1);
                }
                Action::SearchClear => {
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
                    // Use workspace switch for restore
                    self.dispatch_actions(vec![Action::WorkspaceSwitch("default".to_string())]);
                }
                Action::Quit => {
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
                    if let Some(bookmarks) = &self.bookmarks {
                        let _ = bookmarks.remove(&url);
                    }
                }
                Action::ShowBookmarks(query) => {
                    if let Some(bookmarks) = &self.bookmarks {
                        let _results = bookmarks.search(&query, 20).unwrap_or_default();
                        log::info!("Bookmarks search: {} results", _results.len());
                    }
                }
                Action::ShowHistory(query) => {
                    if let Some(history) = &self.history {
                        let _results = history.search(&query, 20).unwrap_or_default();
                        log::info!("History search: {} results", _results.len());
                    }
                }
                Action::CommandBufferChanged => {
                    self.update_suggestions();
                    self.update_hud();
                }
                Action::SuggestionNext => {
                    if !self.suggestions.is_empty() {
                        self.suggestion_index = (self.suggestion_index + 1) % self.suggestions.len();
                        self.update_suggestion_display();
                    }
                }
                Action::SuggestionPrev => {
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
                    self.adjust_zoom(0.1);
                }
                Action::ZoomOut => {
                    self.adjust_zoom(-0.1);
                }
                Action::ZoomReset => {
                    if let Some(focused) = self.layout.focused() {
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
                    let (current_nodes, current_focused) = self.layout.serialize();
                    let current_tiles = self.collect_tile_rows();
                    if let Some(workspace) = &mut self.workspace {
                        match workspace.switch_to(&name, &current_nodes, &current_tiles, current_focused) {
                            Ok(Some(state)) => {
                                // Tear down current
                                for view_id in self.views.all_views() {
                                    if let Some(engine) = &mut self.engine {
                                        engine.destroy_tile(view_id);
                                    }
                                    self.tile_textures.remove(&view_id);
                                }
                                self.views = ViewManager::new();

                                // Restore workspace
                                self.layout = BspLayout::deserialize(self.layout_viewport(), &state.nodes, state.focused);
                                for tile in &state.tiles {
                                    self.views.create_with_id(tile.view_id, &tile.url);
                                    if let Some(engine) = &mut self.engine {
                                        engine.create_tile(tile.view_id, &tile.url);
                                    }
                                }
                                self.update_hud();
                                self.request_redraw();
                            }
                            Ok(None) => {
                                // New empty workspace
                                for view_id in self.views.all_views() {
                                    if let Some(engine) = &mut self.engine {
                                        engine.destroy_tile(view_id);
                                    }
                                    self.tile_textures.remove(&view_id);
                                }
                                self.views = ViewManager::new();
                                self.layout = BspLayout::new(self.layout_viewport());

                                let startup_url = self.config.general.startup_url.clone();
                                let view_id = self.views.create(&startup_url);
                                self.layout.add_first_view(view_id);
                                if let Some(engine) = &mut self.engine {
                                    engine.create_tile(view_id, &startup_url);
                                }
                                self.update_hud();
                                self.request_redraw();
                            }
                            Err(e) => log::error!("Failed to switch workspace: {}", e),
                        }
                    }
                }
                Action::WorkspaceNew(name) => {
                    // WorkspaceSwitch handles creating new workspaces
                    self.dispatch_actions(vec![Action::WorkspaceSwitch(name)]);
                }
                Action::WorkspaceDelete(name) => {
                    if let Some(workspace) = &mut self.workspace {
                        match workspace.delete(&name) {
                            Ok(true) => log::info!("Deleted workspace: {}", name),
                            Ok(false) => log::warn!("Cannot delete active workspace"),
                            Err(e) => log::error!("Failed to delete workspace: {}", e),
                        }
                    }
                }
                Action::WorkspaceList => {
                    if let Some(workspace) = &self.workspace {
                        if let Ok(list) = workspace.list() {
                            for info in &list {
                                log::info!("  {} ({} tiles)", info.name, info.tile_count);
                            }
                        }
                    }
                }
            }
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

            if let Some(focused) = self.layout.focused() {
                if let Some(view) = self.views.get(focused) {
                    hud.set_url_text(&view.url);
                    hud.set_title_text(&view.title);
                }
            }

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
            self.tile_zoom_levels.insert(focused, new_zoom);
            self.apply_zoom(focused, new_zoom);
        }
    }

    fn apply_zoom(&self, view_id: ViewId, zoom: f32) {
        if let Some(engine) = &self.engine {
            let script = format!(
                "(function() {{ document.body.style.zoom = '{}'; }})()",
                zoom
            );
            engine.evaluate_js(view_id, &script, Box::new(|_| {}));
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
    }

    fn click_to_focus(&mut self, x: f32, y: f32) {
        let resolved = self.layout.resolve();
        for (view_id, rect) in resolved {
            if rect.contains(x, y) {
                if self.layout.focused() != Some(view_id) {
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
            let local_event = if let Some(resolved) = self.layout.resolve().into_iter().find(|(id, _)| *id == focused) {
                let rect = resolved.1;
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
            if let Some(engine) = &self.engine {
                engine.send_mouse_event(focused, local_event);
            }
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
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
        Some(CoreKeyEvent { key, state, modifiers })
    }

    /// Upload tile pixels to GL textures.
    unsafe fn update_tile_textures(&mut self, gl: &glow::Context) {
        if let Some(engine) = &self.engine {
            for (view_id, _) in self.layout.resolve() {
                if let Some(pixels) = engine.tile_pixels(view_id) {
                    let (width, height) = pixels.dimensions();
                    let texture = *self.tile_textures.entry(view_id).or_insert_with(|| {
                        gl.create_texture().expect("create texture")
                    });

                    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA as i32,
                        width as i32,
                        height as i32,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(&pixels)),
                    );
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                }
            }
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("Orthogonal")
                    .with_inner_size(winit::dpi::PhysicalSize::new(1280u32, 720u32)),
            )
            .expect("Failed to create window");

        let size = window.inner_size();

        // Initialize Servo engine
        use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
        let display_handle = window.display_handle().unwrap();
        let window_handle = window.window_handle().unwrap();
        let waker = Box::new(WinitWaker { proxy: self.proxy.clone() });

        let mut engine = Engine::new(display_handle, window_handle, (size.width, size.height), waker);

        // Initialize HUD
        let hud = Hud::new(size.width, size.height);

        // Initialize layout
        self.layout = BspLayout::new(Rect::new(0.0, 0.0, size.width as f32, size.height as f32));

        // Open database and initialize managers
        let db_path = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("orthogonal")
            .join("orthogonal.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let mut restored = false;
        if let Ok(conn) = db::open_database(&db_path) {
            self.history = Some(HistoryManager::new(conn.clone(), self.config.history.max_entries));
            self.bookmarks = Some(BookmarkManager::new(conn.clone()));
            let mut wm = WorkspaceManager::new(conn);

            // Try to restore last workspace
            if self.config.general.restore_workspace_on_startup {
                if let Ok(Some(state)) = wm.switch_to("default", &[], &[], None) {
                    if !state.tiles.is_empty() {
                        self.layout = BspLayout::deserialize(
                            Rect::new(0.0, 0.0, size.width as f32, size.height as f32),
                            &state.nodes,
                            state.focused,
                        );
                        for tile in &state.tiles {
                            self.views.create_with_id(tile.view_id, &tile.url);
                            engine.create_tile(tile.view_id, &tile.url);
                        }
                        restored = true;
                    }
                }
                wm.set_active("default");
            }

            self.workspace = Some(wm);
        }

        if !restored {
            let startup_url = &self.config.general.startup_url.clone();
            let view_id = self.views.create(startup_url);
            self.layout.add_first_view(view_id);
            engine.create_tile(view_id, startup_url);
        }

        // Initialize compositor
        let gl = engine.gl_context();
        let compositor = unsafe { Compositor::new(&gl, size.width, size.height) };

        self.window = Some(window);
        self.engine = Some(engine);
        self.hud = Some(hud);
        self.compositor = Some(compositor);
        self.update_hud();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.autosave();
                event_loop.exit();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(core_event) = self.convert_key_event(&event) {
                    let actions = self.input.handle(&core_event);
                    self.dispatch_actions(actions);
                }
            }

            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_position = (position.x, position.y);
                self.dispatch_mouse_event(CoreMouseEvent::Move {
                    x: position.x as f32,
                    y: position.y as f32,
                });
            }

            WindowEvent::MouseInput { state: button_state, button, .. } => {
                let (mx, my) = self.mouse_position;
                let core_button = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    _ => MouseButton::Left,
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
                // Click-to-focus: if clicking inside a non-focused tile, focus it
                if let CoreMouseEvent::Down { x, y, .. } = event {
                    self.click_to_focus(x, y);
                }
                self.dispatch_mouse_event(event);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (mx, my) = self.mouse_position;
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                self.dispatch_mouse_event(CoreMouseEvent::Scroll {
                    x: mx as f32,
                    y: my as f32,
                    delta_x: dx,
                    delta_y: dy,
                });
            }

            WindowEvent::Resized(size) => {
                self.layout.set_viewport(Rect::new(
                    0.0, 0.0,
                    size.width as f32, size.height as f32,
                ));
                // Resize each tile in the engine
                let resolved = self.layout.resolve();
                if let Some(engine) = &mut self.engine {
                    for (view_id, rect) in &resolved {
                        engine.resize_tile(*view_id, rect.width as u32, rect.height as u32);
                    }
                }
                if let Some(hud) = &mut self.hud {
                    hud.resize(size.width, size.height);
                }
                if let (Some(engine), Some(compositor)) = (&self.engine, &mut self.compositor) {
                    let gl = engine.gl_context();
                    unsafe { compositor.resize(&gl, size.width, size.height); }
                }
                self.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                // Paint all tiles first
                if let Some(engine) = &self.engine {
                    let resolved = self.layout.resolve();
                    for (view_id, _) in &resolved {
                        engine.paint_tile(*view_id);
                    }
                }

                // Upload tile pixels to GL textures (borrows self mutably for tile_textures)
                if let Some(engine) = &self.engine {
                    let gl = engine.gl_context();
                    unsafe { self.update_tile_textures(&gl); }
                }

                // Composite and present
                if let (Some(engine), Some(hud), Some(compositor)) = (&self.engine, self.hud.as_mut(), self.compositor.as_ref()) {
                    let gl = engine.gl_context();
                    let resolved = self.layout.resolve();

                    let tile_textures: Vec<(Rect, glow::Texture)> = resolved
                        .iter()
                        .filter_map(|(view_id, rect)| {
                            self.tile_textures.get(view_id).map(|tex| (*rect, *tex))
                        })
                        .collect();

                    let hud_buffer = hud.render();
                    unsafe { compositor.draw(&gl, &tile_textures, hud_buffer); }
                    engine.present();
                }
            }

            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::ServoTick => {
                if let Some(engine) = &mut self.engine {
                    engine.spin();
                }
                self.process_metadata_events();
                self.process_hint_results();
                self.process_search_results();
                self.request_redraw();
            }
        }
    }
}
