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
        let ctx_manager = RenderContextManager::new(display_handle, window_handle, size);

        let mut builder = servo::ServoBuilder::default()
            .event_loop_waker(waker);
        if let Some(prefs) = preferences {
            builder = builder.preferences(prefs);
        }
        let servo = builder.build();

        let (metadata_tx, metadata_rx) = mpsc::channel();
        let delegate = Rc::new(HodeiServoDelegate::new(metadata_tx.clone()));
        servo.set_delegate(delegate);

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
        let offscreen_ctx = self.ctx_manager.create_offscreen(view_id, w, h);
        let url = Url::parse(url_str).unwrap_or_else(|_| {
            Url::parse(&format!("https://{}", url_str)).expect("invalid URL")
        });
        let delegate = Rc::new(HodeiWebViewDelegate::new(view_id, self.metadata_tx.clone()));
        let webview = servo::WebViewBuilder::new(&self.servo, offscreen_ctx.clone())
            .url(url)
            .delegate(delegate)
            .build();
        self.tiles.insert(view_id, TileHandle {
            webview,
            rendering_context: offscreen_ctx,
        });
    }

    pub fn destroy_tile(&mut self, view_id: ViewId) {
        self.tiles.remove(&view_id);
        self.ctx_manager.destroy_offscreen(view_id);
    }

    pub fn resize_tile(&mut self, view_id: ViewId, width: u32, height: u32) {
        if let Some(handle) = self.tiles.get(&view_id) {
            self.ctx_manager.resize_offscreen(view_id, width, height);
            handle.webview.resize(dpi::PhysicalSize::new(width, height));
        }
    }

    pub fn resize_window(&self, width: u32, height: u32) {
        self.ctx_manager.resize_window(width, height);
    }

    pub fn paint_tile(&self, view_id: ViewId) {
        if let Some(handle) = self.tiles.get(&view_id) {
            handle.webview.paint();
        } else {
            log::warn!("paint_tile: no handle for view {:?}", view_id);
        }
    }

    /// Blit the tile's offscreen framebuffer into the currently-bound parent
    /// (window) framebuffer at the given target rectangle. `target_rect` uses
    /// GL convention (origin bottom-left, y grows up).
    ///
    /// Must be called with the window context current and the window FBO bound.
    pub fn blit_tile_to_window(&self, view_id: ViewId, target_rect: euclid::default::Rect<i32>) {
        let Some(handle) = self.tiles.get(&view_id) else { return };
        let Some(callback) = handle.rendering_context.render_to_parent_callback() else {
            log::warn!("blit_tile_to_window: no render_to_parent_callback for view {:?}", view_id);
            return;
        };
        let gl = self.ctx_manager.glow_context();
        callback(&gl, target_rect);
    }

    pub fn tile_size(&self, view_id: ViewId) -> Option<(u32, u32)> {
        let handle = self.tiles.get(&view_id)?;
        let size = handle.rendering_context.size();
        Some((size.width, size.height))
    }

    pub fn navigate(&self, view_id: ViewId, url_str: &str) {
        if let Some(handle) = self.tiles.get(&view_id) {
            if let Ok(url) = Url::parse(url_str) {
                handle.webview.load(url);
            }
        }
    }

    pub fn go_back(&self, view_id: ViewId) {
        if let Some(handle) = self.tiles.get(&view_id) {
            let _ = handle.webview.go_back(1);
        }
    }

    pub fn go_forward(&self, view_id: ViewId) {
        if let Some(handle) = self.tiles.get(&view_id) {
            let _ = handle.webview.go_forward(1);
        }
    }

    pub fn hard_reload(&self, view_id: ViewId) {
        if let Some(handle) = self.tiles.get(&view_id) {
            // Servo reload() doesn't have a cache-bypass flag exposed publicly.
            // Use JS location.reload(true) as a fallback.
            handle.webview.evaluate_javascript(
                "window.location.reload(true)".to_string(),
                Box::new(|_| {}),
            );
        }
    }

    pub fn set_page_zoom(&self, view_id: ViewId, zoom: f32) {
        if let Some(handle) = self.tiles.get(&view_id) {
            handle.webview.set_page_zoom(zoom);
        }
    }

    pub fn set_theme(&self, dark: bool) {
        let theme = if dark { servo::Theme::Dark } else { servo::Theme::Light };
        for handle in self.tiles.values() {
            handle.webview.notify_theme_change(theme);
        }
    }

    pub fn send_input(&self, view_id: ViewId, event: CoreKeyEvent) {
        if let Some(handle) = self.tiles.get(&view_id) {
            let servo_event = events::core_key_to_servo(&event);
            handle.webview.notify_input_event(servo_event);
        }
    }

    pub fn send_mouse_event(&self, view_id: ViewId, event: CoreMouseEvent) {
        if let Some(handle) = self.tiles.get(&view_id) {
            let servo_event = events::core_mouse_to_servo(&event);
            handle.webview.notify_input_event(servo_event);
        }
    }

    pub fn send_click(&self, view_id: ViewId, x: f32, y: f32) {
        if let Some(handle) = self.tiles.get(&view_id) {
            let click = events::click_at(x, y);
            handle.webview.notify_input_event(click);
        }
    }

    pub fn evaluate_js(
        &self,
        view_id: ViewId,
        script: &str,
        callback: Box<dyn FnOnce(Result<String, String>)>,
    ) {
        if let Some(handle) = self.tiles.get(&view_id) {
            handle.webview.evaluate_javascript(
                script.to_string(),
                move |result| {
                    let mapped = result
                        .map(|v| format!("{:?}", v))
                        .map_err(|e| format!("{:?}", e));
                    callback(mapped);
                },
            );
        }
    }

    pub fn spin(&mut self) {
        self.servo.spin_event_loop();
    }

    pub fn drain_metadata_events(&self) -> Vec<MetadataEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.metadata_rx.try_recv() {
            events.push(event);
        }
        events
    }

    pub fn gl_context(&self) -> Arc<glow::Context> {
        self.ctx_manager.glow_context()
    }

    /// Make the window GL context current and bind surfman's window FBO so compositor
    /// draws go to the correct window surface.
    pub fn prepare_window_for_rendering(&self) {
        self.ctx_manager.prepare_window_for_rendering();
    }

    pub fn present(&self) {
        self.ctx_manager.present();
    }

    /// Build Servo `Preferences` with the devtools server enabled.
    pub fn devtools_preferences(tcp_port: u16) -> Preferences {
        let mut prefs = Preferences::default();
        prefs.devtools_server_enabled = true;
        prefs.devtools_server_listen_address = format!("127.0.0.1:{}", tcp_port);
        prefs
    }
}
