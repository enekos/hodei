use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, WindowAdapter};
use slint::{ModelRc, SharedString, VecModel};
use std::rc::Rc;

slint::include_modules!();

// === RGBA pixel type for GL upload ===

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rgba8Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl TargetPixel for Rgba8Pixel {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let inv_sa = 255u16 - color.alpha as u16;
        self.r = (color.red as u16 + self.r as u16 * inv_sa / 255) as u8;
        self.g = (color.green as u16 + self.g as u16 * inv_sa / 255) as u8;
        self.b = (color.blue as u16 + self.b as u16 * inv_sa / 255) as u8;
        self.a = (color.alpha as u16 + self.a as u16 * inv_sa / 255) as u8;
    }

    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

// === Slint Platform ===

struct SlintPlatform {
    window: Rc<MinimalSoftwareWindow>,
    start: std::time::Instant,
}

impl Platform for SlintPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        self.start.elapsed()
    }
}

// === HUD ===

pub struct Hud {
    window: Rc<MinimalSoftwareWindow>,
    hud_instance: HudWindow,
    buffer: Vec<Rgba8Pixel>,
    width: u32,
    height: u32,
}

impl Hud {
    /// Must be called exactly once, before any Slint operations.
    pub fn new(width: u32, height: u32) -> Self {
        log::info!("Hud::new: initializing software renderer {}x{}", width, height);
        let sw_window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        sw_window.set_size(slint::PhysicalSize::new(width, height));

        slint::platform::set_platform(Box::new(SlintPlatform {
            window: sw_window.clone(),
            start: std::time::Instant::now(),
        }))
        .expect("set_platform must be called once");
        log::debug!("Hud::new: Slint platform set");

        // Register the bundled Lucide icon font so `font-family: "lucide"`
        // resolves inside hud.slint. The Slint platform must exist before this
        // runs, hence we only try after `set_platform`.
        register_lucide_font();

        let hud_instance = HudWindow::new().unwrap();
        hud_instance.show().unwrap();
        let buffer = vec![Rgba8Pixel::default(); (width * height) as usize];
        log::info!("Hud::new: initialized with buffer size {} bytes", buffer.len() * 4);

        Self {
            window: sw_window,
            hud_instance,
            buffer,
            width,
            height,
        }
    }

    /// Render the HUD to the internal RGBA buffer. Returns the buffer as bytes,
    /// with straight (non-premultiplied) RGBA. Pixels that Slint's software
    /// renderer fills as "transparent background" come back as opaque black
    /// (0,0,0,255); we convert those to fully transparent so the GL compositor
    /// can blend the HUD over the page content underneath.
    pub fn render(&mut self) -> &[u8] {
        log::trace!("Hud::render: clearing {} pixels", self.buffer.len());
        self.buffer.fill(Rgba8Pixel::default());

        self.window.request_redraw();
        self.window.draw_if_needed(|renderer| {
            renderer.render(&mut self.buffer, self.width as usize);
        });

        // Work around Slint's software renderer writing opaque black where the
        // Window background is transparent. Our HUD uses no pure-black content,
        // so (0,0,0,255) is unambiguously "background".
        let mut transparent_count = 0;
        for p in self.buffer.iter_mut() {
            if p.r == 0 && p.g == 0 && p.b == 0 && p.a == 255 {
                p.a = 0;
                transparent_count += 1;
            }
        }
        log::trace!(
            "Hud::render: {} total pixels, {} transparent (background) pixels",
            self.buffer.len(),
            transparent_count
        );

        unsafe {
            std::slice::from_raw_parts(
                self.buffer.as_ptr() as *const u8,
                self.buffer.len() * 4,
            )
        }
    }

    pub fn set_mode_text(&self, mode: &str) {
        log::trace!("Hud::set_mode_text: {}", mode);
        self.hud_instance.set_mode_text(SharedString::from(mode));
    }

    pub fn set_url_text(&self, url: &str) {
        log::trace!("Hud::set_url_text: {}", url);
        self.hud_instance.set_url_text(SharedString::from(url));
    }

    pub fn set_title_text(&self, title: &str) {
        log::trace!("Hud::set_title_text: {}", title);
        self.hud_instance.set_title_text(SharedString::from(title));
    }

    pub fn set_command_text(&self, text: &str) {
        log::trace!("Hud::set_command_text: {}", text);
        self.hud_instance.set_command_text(SharedString::from(text));
    }

    pub fn set_command_visible(&self, visible: bool) {
        log::trace!("Hud::set_command_visible: {}", visible);
        self.hud_instance.set_command_visible(visible);
    }

    pub fn set_tile_count(&self, count: i32) {
        log::trace!("Hud::set_tile_count: {}", count);
        self.hud_instance.set_tile_count(count);
    }

    pub fn set_focused_index(&self, index: i32) {
        log::trace!("Hud::set_focused_index: {}", index);
        self.hud_instance.set_focused_index(index);
    }

    pub fn set_hints(&self, hints: Vec<(String, f32, f32, bool)>) {
        log::trace!("Hud::set_hints: {} hints", hints.len());
        let model: Vec<HintLabel> = hints
            .into_iter()
            .map(|(label, x, y, active)| HintLabel {
                label: SharedString::from(label),
                x,
                y,
                active,
            })
            .collect();
        let rc = Rc::new(VecModel::from(model));
        self.hud_instance.set_hints(ModelRc::from(rc));
    }

    pub fn set_suggestions(&self, suggestions: Vec<(String, String, bool)>) {
        log::trace!("Hud::set_suggestions: {} suggestions", suggestions.len());
        let model: Vec<SuggestionItem> = suggestions
            .into_iter()
            .map(|(title, url, selected)| SuggestionItem {
                title: SharedString::from(title),
                url: SharedString::from(url),
                selected,
            })
            .collect();
        let rc = Rc::new(VecModel::from(model));
        self.hud_instance.set_suggestions(ModelRc::from(rc));
    }

    pub fn set_suggestions_visible(&self, visible: bool) {
        log::trace!("Hud::set_suggestions_visible: {}", visible);
        self.hud_instance.set_suggestions_visible(visible);
    }

    pub fn set_search_text(&self, text: &str) {
        log::trace!("Hud::set_search_text: {}", text);
        self.hud_instance.set_search_text(SharedString::from(text));
    }

    pub fn set_search_visible(&self, visible: bool) {
        log::trace!("Hud::set_search_visible: {}", visible);
        self.hud_instance.set_search_visible(visible);
    }

    pub fn set_search_info(&self, info: &str) {
        log::trace!("Hud::set_search_info: {}", info);
        self.hud_instance.set_search_info(SharedString::from(info));
    }

    pub fn set_status_text(&self, text: &str) {
        log::trace!("Hud::set_status_text: {}", text);
        self.hud_instance.set_status_text(SharedString::from(text));
    }

    pub fn set_shortcuts_visible(&self, visible: bool) {
        log::trace!("Hud::set_shortcuts_visible: {}", visible);
        self.hud_instance.set_shortcuts_visible(visible);
    }

    pub fn set_loading(&self, v: bool) {
        log::trace!("Hud::set_loading: {}", v);
        self.hud_instance.set_loading(v);
    }

    pub fn set_secure(&self, v: bool) {
        log::trace!("Hud::set_secure: {}", v);
        self.hud_instance.set_secure(v);
    }

    pub fn set_insecure(&self, v: bool) {
        log::trace!("Hud::set_insecure: {}", v);
        self.hud_instance.set_insecure(v);
    }

    pub fn set_bookmarked(&self, v: bool) {
        log::trace!("Hud::set_bookmarked: {}", v);
        self.hud_instance.set_bookmarked(v);
    }

    pub fn set_zoom(&self, zoom: f32) {
        log::trace!("Hud::set_zoom: {}", zoom);
        self.hud_instance.set_zoom(zoom);
    }

    pub fn set_can_back(&self, v: bool) {
        log::trace!("Hud::set_can_back: {}", v);
        self.hud_instance.set_can_back(v);
    }

    pub fn set_can_forward(&self, v: bool) {
        log::trace!("Hud::set_can_forward: {}", v);
        self.hud_instance.set_can_forward(v);
    }

    pub fn clear_hints(&self) {
        log::trace!("Hud::clear_hints");
        let empty: Vec<HintLabel> = vec![];
        let rc = Rc::new(VecModel::from(empty));
        self.hud_instance.set_hints(ModelRc::from(rc));
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        log::info!("Hud::resize: {}x{} -> {}x{}", self.width, self.height, width, height);
        self.width = width;
        self.height = height;
        self.buffer.resize((width * height) as usize, Rgba8Pixel::default());
        self.window.set_size(slint::PhysicalSize::new(width, height));
    }

    pub fn request_redraw(&self) {
        log::trace!("Hud::request_redraw");
        self.window.request_redraw();
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Locate the bundled Lucide TTF relative to the compiled binary or crate.
fn find_lucide_font() -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = std::path::Path::new(&manifest_dir);
        candidates.push(p.join("../../assets/fonts/lucide.ttf"));
        candidates.push(p.join("assets/fonts/lucide.ttf"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("assets/fonts/lucide.ttf"));
            candidates.push(parent.join("../assets/fonts/lucide.ttf"));
            candidates.push(parent.join("../../assets/fonts/lucide.ttf"));
        }
    }
    candidates.push(std::path::PathBuf::from("assets/fonts/lucide.ttf"));

    let candidate_count = candidates.len();
    let found = candidates.into_iter().find(|p| p.exists());
    log::debug!("find_lucide_font: searched {} candidates, found={:?}", candidate_count, found);
    found
}

fn register_lucide_font() {
    let Some(path) = find_lucide_font() else {
        log::warn!("Lucide icon font not found; HUD icon glyphs will render as tofu");
        return;
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("Failed to read Lucide font {}: {}", path.display(), e);
            return;
        }
    };
    use slint::fontique_08::fontique;
    let blob = fontique::Blob::new(std::sync::Arc::new(bytes));
    let mut collection = slint::fontique_08::shared_collection();
    let _ = collection.register_fonts(blob, None);
    log::info!("Registered Lucide icon font from {}", path.display());
}
