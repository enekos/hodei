use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use hodei_core::types::ViewId;
use servo::RenderingContext;
use glow::HasContext;

/// Drain any pending GL errors. Surfman has a `debug_assert_eq!(gl.get_error(), NO_ERROR)`
/// after binding a new IOSurface-backed texture on macOS; stray errors left by Servo's
/// paint or the HUD compositor trip it during resize. Drain before any surfman operation
/// that we know performs that assertion.
fn drain_gl_errors(gl: &glow::Context) {
    unsafe {
        let mut n = 0;
        loop {
            let err = gl.get_error();
            if err == glow::NO_ERROR || n > 16 {
                break;
            }
            log::debug!("drain_gl_errors: cleared pending gl error {:#x}", err);
            n += 1;
        }
    }
}

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
        log::debug!(
            "RenderContextManager::new: display_handle={:?} window_handle={:?} size={:?}",
            display_handle,
            window_handle,
            size
        );
        let window_ctx = Rc::new(
            servo::WindowRenderingContext::new(
                display_handle,
                window_handle,
                dpi::PhysicalSize::new(size.0, size.1),
            )
            .expect("Failed to create WindowRenderingContext"),
        );
        log::info!("RenderContextManager: created window rendering context {:?}x{:?}", size.0, size.1);
        Self {
            window_ctx,
            offscreen: HashMap::new(),
        }
    }

    pub fn create_offscreen(&mut self, view_id: ViewId, width: u32, height: u32) -> Rc<servo::OffscreenRenderingContext> {
        log::debug!("RenderContextManager::create_offscreen: view_id={:?} size={}x{}", view_id, width, height);
        let ctx = Rc::new(
            self.window_ctx.offscreen_context(dpi::PhysicalSize::new(width, height))
        );
        self.offscreen.insert(view_id, ctx.clone());
        log::info!("RenderContextManager: created offscreen context for view {:?} ({}x{})", view_id, width, height);
        ctx
    }

    pub fn destroy_offscreen(&mut self, view_id: ViewId) {
        log::debug!("RenderContextManager::destroy_offscreen: view_id={:?}", view_id);
        let removed = self.offscreen.remove(&view_id).is_some();
        if removed {
            log::info!("RenderContextManager: destroyed offscreen context for view {:?}", view_id);
        } else {
            log::warn!("RenderContextManager::destroy_offscreen: no offscreen context for view {:?}", view_id);
        }
    }

    pub fn resize_offscreen(&mut self, view_id: ViewId, width: u32, height: u32) {
        log::debug!("RenderContextManager::resize_offscreen: view_id={:?} new_size={}x{}", view_id, width, height);
        if let Some(ctx) = self.offscreen.get(&view_id) {
            drain_gl_errors(&self.window_ctx.glow_gl_api());
            ctx.resize(dpi::PhysicalSize::new(width, height));
            log::info!("RenderContextManager: resized offscreen context for view {:?} to {}x{}", view_id, width, height);
        } else {
            log::warn!("RenderContextManager::resize_offscreen: no offscreen context for view {:?}", view_id);
        }
    }

    pub fn offscreen_for(&self, view_id: ViewId) -> Option<Rc<servo::OffscreenRenderingContext>> {
        let result = self.offscreen.get(&view_id).cloned();
        log::trace!("RenderContextManager::offscreen_for: view_id={:?} found={}", view_id, result.is_some());
        result
    }

    pub fn glow_context(&self) -> Arc<glow::Context> {
        log::trace!("RenderContextManager::glow_context");
        self.window_ctx.glow_gl_api()
    }

    pub fn prepare_window_for_rendering(&self) {
        log::trace!("RenderContextManager::prepare_window_for_rendering: making window context current");
        if let Err(e) = self.window_ctx.make_current() {
            log::error!("prepare_window_for_rendering: make_current failed: {:?}", e);
        }
        self.window_ctx.prepare_for_rendering();
        log::trace!("RenderContextManager::prepare_window_for_rendering: window FBO bound");
    }

    pub fn present(&self) {
        log::trace!("RenderContextManager::present: swapping buffers");
        self.window_ctx.present();
    }

    pub fn resize_window(&self, width: u32, height: u32) {
        log::debug!("RenderContextManager::resize_window: new_size={}x{}", width, height);
        drain_gl_errors(&self.window_ctx.glow_gl_api());
        self.window_ctx.resize(dpi::PhysicalSize::new(width, height));
        log::info!("RenderContextManager: resized window context to {}x{}", width, height);
    }
}
