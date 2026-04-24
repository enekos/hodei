mod app;
mod devtools_bridge;

use winit::event_loop::{ControlFlow, EventLoop};

#[derive(Debug)]
pub enum UserEvent {
    ServoTick,
    /// Emitted by Slint HUD callbacks — drained on the main thread so we can
    /// borrow `self` mutably.
    HudAction(hodei_core::input::Action),
}

fn main() {
    env_logger::init();
    let pid = std::process::id();
    let args: Vec<String> = std::env::args().collect();
    log::info!("Starting Hodei v0.1.0 (pid={}) args={:?}", pid, args);

    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    log::debug!("EventLoop created with ControlFlow::Wait");

    let proxy = event_loop.create_proxy();
    let mut app = app::App::new(proxy);
    log::info!("App initialized, entering event loop");
    event_loop.run_app(&mut app).expect("Event loop error");
    log::info!("Event loop exited, shutting down");
}
