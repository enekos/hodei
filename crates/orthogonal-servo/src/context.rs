use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use orthogonal_core::types::ViewId;
use servo::RenderingContext;

/// Manages the WindowRenderingContext and per-tile OffscreenRenderingContexts.
pub struct RenderContextManager {
    window_ctx: Rc<servo::WindowRenderingContext>,
    offscreen: HashMap<ViewId, Rc<servo::OffscreenRenderingContext>>,
}

impl RenderContextManager {
    pub fn new(
        display_handle: raw_window_handle::DisplayHandle<'_>,
        window_handle: raw_window_handle::WindowHandle<'_>,
        size: (u32, u32),
    ) -> Self {
        let window_ctx = Rc::new(
            servo::WindowRenderingContext::new(
                display_handle,
                window_handle,
                dpi::PhysicalSize::new(size.0, size.1),
            )
            .expect("Failed to create WindowRenderingContext"),
        );
        Self {
            window_ctx,
            offscreen: HashMap::new(),
        }
    }

    pub fn create_offscreen(&mut self, view_id: ViewId, width: u32, height: u32) -> Rc<servo::OffscreenRenderingContext> {
        let ctx = Rc::new(
            self.window_ctx.offscreen_context(dpi::PhysicalSize::new(width, height))
        );
        self.offscreen.insert(view_id, ctx.clone());
        ctx
    }

    pub fn destroy_offscreen(&mut self, view_id: ViewId) {
        self.offscreen.remove(&view_id);
    }

    pub fn resize_offscreen(&mut self, view_id: ViewId, width: u32, height: u32) {
        if let Some(ctx) = self.offscreen.get(&view_id) {
            ctx.resize(dpi::PhysicalSize::new(width, height));
        }
    }

    pub fn offscreen_for(&self, view_id: ViewId) -> Option<Rc<servo::OffscreenRenderingContext>> {
        self.offscreen.get(&view_id).cloned()
    }

    pub fn glow_context(&self) -> Arc<glow::Context> {
        self.window_ctx.glow_gl_api()
    }

    pub fn present(&self) {
        self.window_ctx.present();
    }
}
