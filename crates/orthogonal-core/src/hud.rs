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
        let sw_window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        sw_window.set_size(slint::PhysicalSize::new(width, height));

        slint::platform::set_platform(Box::new(SlintPlatform {
            window: sw_window.clone(),
            start: std::time::Instant::now(),
        }))
        .expect("set_platform must be called once");

        let hud_instance = HudWindow::new().unwrap();
        let buffer = vec![Rgba8Pixel::default(); (width * height) as usize];

        Self {
            window: sw_window,
            hud_instance,
            buffer,
            width,
            height,
        }
    }

    /// Render the HUD to the internal RGBA buffer. Returns the buffer as bytes.
    pub fn render(&mut self) -> &[u8] {
        // Clear buffer to transparent
        self.buffer.fill(Rgba8Pixel::default());

        self.window.draw_if_needed(|renderer| {
            renderer.render(&mut self.buffer, self.width as usize);
        });

        // Safety: Rgba8Pixel is #[repr(C)] with 4 u8 fields
        unsafe {
            std::slice::from_raw_parts(
                self.buffer.as_ptr() as *const u8,
                self.buffer.len() * 4,
            )
        }
    }

    pub fn set_mode_text(&self, mode: &str) {
        self.hud_instance.set_mode_text(SharedString::from(mode));
    }

    pub fn set_url_text(&self, url: &str) {
        self.hud_instance.set_url_text(SharedString::from(url));
    }

    pub fn set_title_text(&self, title: &str) {
        self.hud_instance.set_title_text(SharedString::from(title));
    }

    pub fn set_command_text(&self, text: &str) {
        self.hud_instance.set_command_text(SharedString::from(text));
    }

    pub fn set_command_visible(&self, visible: bool) {
        self.hud_instance.set_command_visible(visible);
    }

    pub fn set_tile_count(&self, count: i32) {
        self.hud_instance.set_tile_count(count);
    }

    pub fn set_focused_index(&self, index: i32) {
        self.hud_instance.set_focused_index(index);
    }

    pub fn set_hints(&self, hints: Vec<(String, f32, f32, bool)>) {
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

    pub fn clear_hints(&self) {
        let empty: Vec<HintLabel> = vec![];
        let rc = Rc::new(VecModel::from(empty));
        self.hud_instance.set_hints(ModelRc::from(rc));
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.buffer.resize((width * height) as usize, Rgba8Pixel::default());
        self.window.set_size(slint::PhysicalSize::new(width, height));
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}
