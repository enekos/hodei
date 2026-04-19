use hodei_core::types::*;

/// Convert a core key event to a Servo InputEvent.
pub fn core_key_to_servo(event: &CoreKeyEvent) -> servo::InputEvent {
    let key = match event.key {
        CoreKey::Char(c) => servo::Key::Character(c.to_string()),
        CoreKey::Escape => servo::Key::Named(servo::NamedKey::Escape),
        CoreKey::Enter => servo::Key::Named(servo::NamedKey::Enter),
        CoreKey::Backspace => servo::Key::Named(servo::NamedKey::Backspace),
        CoreKey::Tab => servo::Key::Named(servo::NamedKey::Tab),
        CoreKey::Left => servo::Key::Named(servo::NamedKey::ArrowLeft),
        CoreKey::Right => servo::Key::Named(servo::NamedKey::ArrowRight),
        CoreKey::Up => servo::Key::Named(servo::NamedKey::ArrowUp),
        CoreKey::Down => servo::Key::Named(servo::NamedKey::ArrowDown),
        CoreKey::Home => servo::Key::Named(servo::NamedKey::Home),
        CoreKey::End => servo::Key::Named(servo::NamedKey::End),
        CoreKey::PageUp => servo::Key::Named(servo::NamedKey::PageUp),
        CoreKey::PageDown => servo::Key::Named(servo::NamedKey::PageDown),
    };

    let state = match event.state {
        KeyState::Pressed => servo::KeyState::Down,
        KeyState::Released => servo::KeyState::Up,
    };

    let mut modifiers = servo::Modifiers::empty();
    if event.modifiers.ctrl {
        modifiers |= servo::Modifiers::CONTROL;
    }
    if event.modifiers.alt {
        modifiers |= servo::Modifiers::ALT;
    }
    if event.modifiers.shift {
        modifiers |= servo::Modifiers::SHIFT;
    }
    if event.modifiers.meta {
        modifiers |= servo::Modifiers::META;
    }

    let keyboard_event = servo::KeyboardEvent::new_without_event(
        state,
        key,
        servo::Code::Unidentified,
        servo::Location::Standard,
        modifiers,
        false,
        false,
    );

    servo::InputEvent::Keyboard(keyboard_event)
}

/// Generate a click InputEvent at the given coordinates.
pub fn click_at(x: f32, y: f32) -> servo::InputEvent {
    let point = servo::DevicePoint::new(x as f32, y as f32);
    let webview_point = servo::WebViewPoint::from(point);
    servo::InputEvent::MouseButton(servo::MouseButtonEvent::new(
        servo::MouseButtonAction::Down,
        servo::MouseButton::Left,
        webview_point,
    ))
}

/// Convert a core mouse event to a Servo InputEvent.
pub fn core_mouse_to_servo(event: &CoreMouseEvent) -> servo::InputEvent {
    match event {
        CoreMouseEvent::Move { x, y } => {
            let point = servo::DevicePoint::new(*x as f32, *y as f32);
            let webview_point = servo::WebViewPoint::from(point);
            servo::InputEvent::MouseMove(servo::MouseMoveEvent::new(webview_point))
        }
        CoreMouseEvent::Down { x, y, button } => {
            let point = servo::DevicePoint::new(*x as f32, *y as f32);
            let webview_point = servo::WebViewPoint::from(point);
            let servo_button = mouse_button_to_servo(button);
            servo::InputEvent::MouseButton(servo::MouseButtonEvent::new(
                servo::MouseButtonAction::Down,
                servo_button,
                webview_point,
            ))
        }
        CoreMouseEvent::Up { x, y, button } => {
            let point = servo::DevicePoint::new(*x as f32, *y as f32);
            let webview_point = servo::WebViewPoint::from(point);
            let servo_button = mouse_button_to_servo(button);
            servo::InputEvent::MouseButton(servo::MouseButtonEvent::new(
                servo::MouseButtonAction::Up,
                servo_button,
                webview_point,
            ))
        }
        CoreMouseEvent::Scroll { x, y, delta_x, delta_y } => {
            let point = servo::DevicePoint::new(*x as f32, *y as f32);
            let webview_point = servo::WebViewPoint::from(point);
            servo::InputEvent::Wheel(servo::WheelEvent::new(
                servo::WheelDelta {
                    x: *delta_x as f64,
                    y: *delta_y as f64,
                    z: 0.0,
                    mode: servo::WheelMode::DeltaPixel,
                },
                webview_point,
            ))
        }
    }
}

fn mouse_button_to_servo(button: &MouseButton) -> servo::MouseButton {
    match button {
        MouseButton::Left => servo::MouseButton::Left,
        MouseButton::Right => servo::MouseButton::Right,
        MouseButton::Middle => servo::MouseButton::Middle,
    }
}
