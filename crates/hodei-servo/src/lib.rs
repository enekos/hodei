pub mod context;
pub mod delegate;
pub mod events;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;

use hodei_core::types::*;
use servo::RenderingContext;
use url::Url;

pub use context::RenderContextManager;
pub use delegate::{HodeiServoDelegate, HodeiWebViewDelegate};

// Re-export types needed by hodei-app without depending on servo directly.
pub use servo::EventLoopWaker as ServoEventLoopWaker;
pub use servo::{Preferences, PrefValue};

/// The Servo engine facade. Wraps all Servo interactions.
pub struct Engine {
    servo: servo::Servo,
    ctx_manager: RenderContextManager,
    tiles: HashMap<ViewId, TileHandle>,
    metadata_tx: mpsc::Sender<MetadataEvent>,
    metadata_rx: mpsc::Receiver<MetadataEvent>,
}

struct TileHandle {
    webview: servo::WebView,
    rendering_context: Rc<servo::OffscreenRenderingContext>,
}

impl Engine {
    /// Create a new Engine. Call from the main thread with a valid window.
    pub fn new(
        display_handle: raw_window_handle::DisplayHandle<'_>,
        window_handle: raw_window_handle::WindowHandle<'_>,
        size: (u32, u32),
        waker: Box<dyn servo::EventLoopWaker>,
        preferences: Option<Preferences>,
    ) -> Self {
        log::info!("Engine::new: creating render context manager size={:?}", size);
        let ctx_manager = RenderContextManager::new(display_handle, window_handle, size);

        let mut builder = servo::ServoBuilder::default()
            .event_loop_waker(waker);
        if let Some(prefs) = preferences {
            log::debug!("Engine::new: applying custom preferences");
            builder = builder.preferences(prefs);
        }
        log::debug!("Engine::new: building servo instance");
        let servo = builder.build();
        log::info!("Engine::new: servo instance built");

        let (metadata_tx, metadata_rx) = mpsc::channel();
        let delegate = Rc::new(HodeiServoDelegate::new(metadata_tx.clone()));
        servo.set_delegate(delegate);
        log::debug!("Engine::new: servo delegate set");

        Self {
            servo,
            ctx_manager,
            tiles: HashMap::new(),
            metadata_tx,
            metadata_rx,
        }
    }

    pub fn create_tile(&mut self, view_id: ViewId, url_str: &str, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        log::info!("Engine::create_tile: view_id={:?} url={} size={}x{}", view_id, url_str, w, h);
        let offscreen_ctx = self.ctx_manager.create_offscreen(view_id, w, h);
        let url = Url::parse(url_str).unwrap_or_else(|_| {
            log::debug!("Engine::create_tile: URL parse failed, trying https:// fallback");
            Url::parse(&format!("https://{}", url_str)).expect("invalid URL")
        });
        let delegate = Rc::new(HodeiWebViewDelegate::new(view_id, self.metadata_tx.clone()));
        log::debug!("Engine::create_tile: building webview for {:?}", view_id);
        let webview = servo::WebViewBuilder::new(&self.servo, offscreen_ctx.clone())
            .url(url)
            .delegate(delegate)
            .build();
        self.tiles.insert(view_id, TileHandle {
            webview,
            rendering_context: offscreen_ctx,
        });
        log::info!("Engine::create_tile: view {:?} created successfully (total tiles: {})", view_id, self.tiles.len());
    }

    pub fn destroy_tile(&mut self, view_id: ViewId) {
        log::info!("Engine::destroy_tile: view_id={:?}", view_id);
        let removed = self.tiles.remove(&view_id).is_some();
        self.ctx_manager.destroy_offscreen(view_id);
        if removed {
            log::info!("Engine::destroy_tile: view {:?} destroyed (remaining tiles: {})", view_id, self.tiles.len());
        } else {
            log::warn!("Engine::destroy_tile: view {:?} did not exist", view_id);
        }
    }

    pub fn resize_tile(&mut self, view_id: ViewId, width: u32, height: u32) {
        log::debug!("Engine::resize_tile: view_id={:?} new_size={}x{}", view_id, width, height);
        if let Some(handle) = self.tiles.get(&view_id) {
            self.ctx_manager.resize_offscreen(view_id, width, height);
            handle.webview.resize(dpi::PhysicalSize::new(width, height));
            log::info!("Engine::resize_tile: view {:?} resized to {}x{}", view_id, width, height);
        } else {
            log::warn!("Engine::resize_tile: no handle for view {:?}", view_id);
        }
    }

    pub fn resize_window(&self, width: u32, height: u32) {
        log::info!("Engine::resize_window: new_size={}x{}", width, height);
        self.ctx_manager.resize_window(width, height);
    }

    pub fn paint_tile(&self, view_id: ViewId) {
        log::trace!("Engine::paint_tile: view_id={:?}", view_id);
        if let Some(handle) = self.tiles.get(&view_id) {
            handle.webview.paint();
            log::trace!("Engine::paint_tile: view {:?} painted", view_id);
        } else {
            log::warn!("Engine::paint_tile: no handle for view {:?}", view_id);
        }
    }

    /// Blit the tile's offscreen framebuffer into the currently-bound parent
    /// (window) framebuffer at the given target rectangle. `target_rect` uses
    /// GL convention (origin bottom-left, y grows up).
    ///
    /// Must be called with the window context current and the window FBO bound.
    pub fn blit_tile_to_window(&self, view_id: ViewId, target_rect: euclid::default::Rect<i32>) {
        log::trace!("Engine::blit_tile_to_window: view_id={:?} target={:?}", view_id, target_rect);
        let Some(handle) = self.tiles.get(&view_id) else {
            log::warn!("Engine::blit_tile_to_window: no handle for view {:?}", view_id);
            return;
        };
        let Some(callback) = handle.rendering_context.render_to_parent_callback() else {
            log::warn!("Engine::blit_tile_to_window: no render_to_parent_callback for view {:?}", view_id);
            return;
        };
        let gl = self.ctx_manager.glow_context();
        callback(&gl, target_rect);
        log::trace!("Engine::blit_tile_to_window: view {:?} blitted to {:?}", view_id, target_rect);
    }

    pub fn tile_size(&self, view_id: ViewId) -> Option<(u32, u32)> {
        let handle = self.tiles.get(&view_id)?;
        let size = handle.rendering_context.size();
        log::trace!("Engine::tile_size: view_id={:?} size={}x{}", view_id, size.width, size.height);
        Some((size.width, size.height))
    }

    pub fn navigate(&self, view_id: ViewId, url_str: &str) {
        log::info!("Engine::navigate: view_id={:?} url={}", view_id, url_str);
        if let Some(handle) = self.tiles.get(&view_id) {
            if let Ok(url) = Url::parse(url_str) {
                handle.webview.load(url);
                log::info!("Engine::navigate: view {:?} loaded url {}", view_id, url_str);
            } else {
                log::warn!("Engine::navigate: invalid URL '{}' for view {:?}", url_str, view_id);
            }
        } else {
            log::warn!("Engine::navigate: no handle for view {:?}", view_id);
        }
    }

    pub fn go_back(&self, view_id: ViewId) {
        log::debug!("Engine::go_back: view_id={:?}", view_id);
        if let Some(handle) = self.tiles.get(&view_id) {
            let _result = handle.webview.go_back(1);
            log::debug!("Engine::go_back: view_id={:?}", view_id);
        } else {
            log::warn!("Engine::go_back: no handle for view {:?}", view_id);
        }
    }

    pub fn go_forward(&self, view_id: ViewId) {
        log::debug!("Engine::go_forward: view_id={:?}", view_id);
        if let Some(handle) = self.tiles.get(&view_id) {
            let _result = handle.webview.go_forward(1);
            log::debug!("Engine::go_forward: view_id={:?}", view_id);
        } else {
            log::warn!("Engine::go_forward: no handle for view {:?}", view_id);
        }
    }

    pub fn hard_reload(&self, view_id: ViewId) {
        log::info!("Engine::hard_reload: view_id={:?}", view_id);
        if let Some(handle) = self.tiles.get(&view_id) {
            // Servo reload() doesn't have a cache-bypass flag exposed publicly.
            // Use JS location.reload(true) as a fallback.
            handle.webview.evaluate_javascript(
                "window.location.reload(true)".to_string(),
                Box::new(|_| {}),
            );
        } else {
            log::warn!("Engine::hard_reload: no handle for view {:?}", view_id);
        }
    }

    pub fn set_page_zoom(&self, view_id: ViewId, zoom: f32) {
        log::debug!("Engine::set_page_zoom: view_id={:?} zoom={}", view_id, zoom);
        if let Some(handle) = self.tiles.get(&view_id) {
            handle.webview.set_page_zoom(zoom);
        } else {
            log::warn!("Engine::set_page_zoom: no handle for view {:?}", view_id);
        }
    }

    pub fn set_theme(&self, dark: bool) {
        let theme = if dark { servo::Theme::Dark } else { servo::Theme::Light };
        log::info!("Engine::set_theme: dark={} (affecting {} tiles)", dark, self.tiles.len());
        for handle in self.tiles.values() {
            handle.webview.notify_theme_change(theme);
        }
    }

    pub fn send_input(&self, view_id: ViewId, event: CoreKeyEvent) {
        log::trace!("Engine::send_input: view_id={:?} event={:?}", view_id, event);
        if let Some(handle) = self.tiles.get(&view_id) {
            let servo_event = events::core_key_to_servo(&event);
            handle.webview.notify_input_event(servo_event);
        } else {
            log::warn!("Engine::send_input: no handle for view {:?}", view_id);
        }
    }

    pub fn send_mouse_event(&self, view_id: ViewId, event: CoreMouseEvent) {
        log::trace!("Engine::send_mouse_event: view_id={:?} event={:?}", view_id, event);
        if let Some(handle) = self.tiles.get(&view_id) {
            let servo_event = events::core_mouse_to_servo(&event);
            handle.webview.notify_input_event(servo_event);
        } else {
            log::warn!("Engine::send_mouse_event: no handle for view {:?}", view_id);
        }
    }

    pub fn send_click(&self, view_id: ViewId, x: f32, y: f32) {
        log::debug!("Engine::send_click: view_id={:?} x={} y={}", view_id, x, y);
        if let Some(handle) = self.tiles.get(&view_id) {
            let click = events::click_at(x, y);
            handle.webview.notify_input_event(click);
        } else {
            log::warn!("Engine::send_click: no handle for view {:?}", view_id);
        }
    }

    pub fn evaluate_js(
        &self,
        view_id: ViewId,
        script: &str,
        callback: Box<dyn FnOnce(Result<String, String>)>,
    ) {
        log::debug!("Engine::evaluate_js: view_id={:?} script_len={}", view_id, script.len());
        if let Some(handle) = self.tiles.get(&view_id) {
            handle.webview.evaluate_javascript(
                script.to_string(),
                move |result| {
                    let mapped = result
                        .map(|v| format!("{:?}", v))
                        .map_err(|e| format!("{:?}", e));
                    log::trace!("Engine::evaluate_js: view_id={:?} result={:?}", view_id, mapped.is_ok());
                    callback(mapped);
                },
            );
        } else {
            log::warn!("Engine::evaluate_js: no handle for view {:?}", view_id);
        }
    }

    pub fn spin(&mut self) {
        log::trace!("Engine::spin: spinning event loop");
        self.servo.spin_event_loop();
    }

    pub fn drain_metadata_events(&self) -> Vec<MetadataEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.metadata_rx.try_recv() {
            log::trace!("Engine::drain_metadata_events: received {:?}", event);
            events.push(event);
        }
        log::trace!("Engine::drain_metadata_events: drained {} events", events.len());
        events
    }

    pub fn gl_context(&self) -> Arc<glow::Context> {
        log::trace!("Engine::gl_context");
        self.ctx_manager.glow_context()
    }

    /// Make the window GL context current and bind surfman's window FBO so compositor
    /// draws go to the correct window surface.
    pub fn prepare_window_for_rendering(&self) {
        log::trace!("Engine::prepare_window_for_rendering");
        self.ctx_manager.prepare_window_for_rendering();
    }

    pub fn present(&self) {
        log::trace!("Engine::present");
        self.ctx_manager.present();
    }

    /// Build Servo `Preferences` with the devtools server enabled.
    pub fn devtools_preferences(tcp_port: u16) -> Preferences {
        log::info!("Engine::devtools_preferences: tcp_port={}", tcp_port);
        let mut prefs = Preferences::default();
        prefs.devtools_server_enabled = true;
        prefs.devtools_server_listen_address = format!("127.0.0.1:{}", tcp_port);
        prefs
    }
}
