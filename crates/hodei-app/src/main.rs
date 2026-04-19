mod app;

use winit::event_loop::{ControlFlow, EventLoop};

#[derive(Debug)]
pub enum UserEvent {
    ServoTick,
}

fn main() {
    env_logger::init();
    log::info!("Starting Orthogonal v0.1.0");

    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();
    let mut app = app::App::new(proxy);
    event_loop.run_app(&mut app).expect("Event loop error");
}
