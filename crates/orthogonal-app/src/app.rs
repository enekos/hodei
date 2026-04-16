use std::sync::mpsc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowId};
use glow::HasContext;

use orthogonal_core::compositor::Compositor;
use orthogonal_core::hint;
use orthogonal_core::hud::Hud;
use orthogonal_core::input::{Action, InputRouter, Mode};
use orthogonal_core::layout::BspLayout;
use orthogonal_core::session::SessionManager;
use orthogonal_core::types::*;
use orthogonal_core::view::ViewManager;
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
    window: Option<Window>,
    engine: Option<Engine>,
    hud: Option<Hud>,
    compositor: Option<Compositor>,
    layout: BspLayout,
    views: ViewManager,
    input: InputRouter,
    modifiers: winit::keyboard::ModifiersState,
    // GL textures for each tile (uploaded from Servo offscreen pixels)
    tile_textures: std::collections::HashMap<ViewId, glow::Texture>,
    session: Option<SessionManager>,
    // Hint mode state
    hint_elements: Vec<HintElement>,
    hint_labels: Vec<String>,
    hint_result_tx: mpsc::Sender<String>,
    hint_result_rx: mpsc::Receiver<String>,
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let (hint_result_tx, hint_result_rx) = mpsc::channel();
        Self {
            proxy,
            window: None,
            engine: None,
            hud: None,
            compositor: None,
            layout: BspLayout::new(Rect::default()),
            views: ViewManager::new(),
            input: InputRouter::new(),
            modifiers: winit::keyboard::ModifiersState::empty(),
            tile_textures: std::collections::HashMap::new(),
            session: None,
            hint_elements: Vec::new(),
            hint_labels: Vec::new(),
            hint_result_tx,
            hint_result_rx,
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
                Action::Navigate(url) => {
                    if let Some(focused) = self.layout.focused() {
                        if let Some(engine) = &self.engine {
                            engine.navigate(focused, &url);
                        }
                    }
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
                Action::SaveSession => {
                    if let Some(session) = &self.session {
                        let (nodes, focused) = self.layout.serialize();
                        let tiles = self.collect_tile_rows();
                        if let Err(e) = session.save("manual", &nodes, &tiles, focused) {
                            log::error!("Failed to save session: {}", e);
                        } else {
                            log::info!("Session saved");
                        }
                    }
                }
                Action::RestoreSession => {
                    if let Some(session) = &self.session {
                        match session.restore("manual") {
                            Ok(Some((nodes, tiles, focused))) => {
                                // Tear down current tiles
                                for view_id in self.views.all_views() {
                                    if let Some(engine) = &mut self.engine {
                                        engine.destroy_tile(view_id);
                                    }
                                    self.tile_textures.remove(&view_id);
                                }
                                self.views = ViewManager::new();

                                // Rebuild layout from saved state
                                self.layout = BspLayout::deserialize(
                                    self.layout_viewport(),
                                    &nodes,
                                    focused,
                                );

                                // Recreate views and engine tiles
                                for tile in &tiles {
                                    self.views.create_with_id(tile.view_id, &tile.url);
                                    if let Some(engine) = &mut self.engine {
                                        engine.create_tile(tile.view_id, &tile.url);
                                    }
                                }

                                self.update_hud();
                                self.request_redraw();
                                log::info!("Session restored");
                            }
                            Ok(None) => log::info!("No session to restore"),
                            Err(e) => log::error!("Failed to restore session: {}", e),
                        }
                    }
                }
                Action::Quit => {
                    self.autosave();
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
        }
    }

    fn update_hud(&self) {
        if let Some(hud) = &self.hud {
            let mode_str = match self.input.mode() {
                Mode::Normal => "NORMAL",
                Mode::Insert => "INSERT",
                Mode::Command { .. } => "COMMAND",
                Mode::Hint { .. } => "HINT",
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
                        view.url = url;
                        needs_hud_update = true;
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

    fn autosave(&self) {
        if let Some(session) = &self.session {
            let (nodes, focused) = self.layout.serialize();
            let tiles = self.collect_tile_rows();
            if let Err(e) = session.autosave(&nodes, &tiles, focused) {
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

        // Initialize session manager
        let db_path = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("orthogonal")
            .join("sessions.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        match SessionManager::open(&db_path) {
            Ok(sm) => self.session = Some(sm),
            Err(e) => log::error!("Failed to open session database: {}", e),
        }

        // Create first tile
        let view_id = self.views.create("https://servo.org");
        self.layout.add_first_view(view_id);
        engine.create_tile(view_id, "https://servo.org");

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
                self.request_redraw();
            }
        }
    }
}
