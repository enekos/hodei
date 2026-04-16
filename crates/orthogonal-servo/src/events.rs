use orthogonal_core::types::*;

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
