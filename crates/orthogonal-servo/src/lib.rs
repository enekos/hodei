pub mod context;
pub mod delegate;
pub mod events;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;

use orthogonal_core::types::*;
use servo::RenderingContext;
use url::Url;

pub use context::RenderContextManager;
pub use delegate::{OrthoServoDelegate, OrthoWebViewDelegate};

// Re-export types needed by orthogonal-app without depending on servo directly.
pub use servo::EventLoopWaker as ServoEventLoopWaker;

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
    ) -> Self {
        let ctx_manager = RenderContextManager::new(display_handle, window_handle, size);

        let servo = servo::ServoBuilder::default()
            .event_loop_waker(waker)
            .build();

        let delegate = Rc::new(OrthoServoDelegate);
        servo.set_delegate(delegate);

        let (metadata_tx, metadata_rx) = mpsc::channel();

        Self {
            servo,
            ctx_manager,
            tiles: HashMap::new(),
            metadata_tx,
            metadata_rx,
        }
    }

    pub fn create_tile(&mut self, view_id: ViewId, url_str: &str) {
        let offscreen_ctx = self.ctx_manager.create_offscreen(view_id, 800, 600);
        let url = Url::parse(url_str).unwrap_or_else(|_| {
            Url::parse(&format!("https://{}", url_str)).expect("invalid URL")
        });
        let delegate = Rc::new(OrthoWebViewDelegate::new(view_id, self.metadata_tx.clone()));
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

    pub fn paint_tile(&self, view_id: ViewId) {
        if let Some(handle) = self.tiles.get(&view_id) {
            handle.webview.paint();
        }
    }

    /// Read the painted tile pixels. The caller can upload these to a GL texture.
    pub fn tile_pixels(&self, view_id: ViewId) -> Option<image::RgbaImage> {
        let handle = self.tiles.get(&view_id)?;
        let size = handle.rendering_context.size();
        let rect = servo::DeviceIntRect::from_origin_and_size(
            servo::DeviceIntPoint::new(0, 0),
            servo::DeviceIntSize::new(size.width as i32, size.height as i32),
        );
        handle.rendering_context.read_to_image(rect)
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

    pub fn send_input(&self, view_id: ViewId, event: CoreKeyEvent) {
        if let Some(handle) = self.tiles.get(&view_id) {
            let servo_event = events::core_key_to_servo(&event);
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

    pub fn present(&self) {
        self.ctx_manager.present();
    }
}
