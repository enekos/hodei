use crate::types::*;

// === Modes ===

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Command { buffer: String },
    Hint { filter: String, labels: Vec<String> },
}

// === Actions ===

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    FocusNeighbor(Direction),
    SplitView(SplitDirection),
    CloseView,
    ResizeSplit(Direction, f32),
    ForwardToServo(CoreKeyEvent),
    Navigate(String),
    Back,
    Forward,
    Reload,
    EnterHintMode,
    HintCharTyped(char),
    ActivateHint(String),
    EnterInsert,
    EnterCommand,
    ExitToNormal,
    SaveSession,
    RestoreSession,
    Quit,
}

// === Router ===

pub struct InputRouter {
    mode: Mode,
}

impl InputRouter {
    pub fn new() -> Self {
        Self { mode: Mode::Normal }
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn handle(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        if event.state != KeyState::Pressed {
            return vec![];
        }
        match &self.mode {
            Mode::Normal => self.handle_normal(event),
            Mode::Insert => self.handle_insert(event),
            Mode::Command { .. } => self.handle_command(event),
            Mode::Hint { .. } => self.handle_hint(event),
        }
    }

    /// Called by app after hint elements are fetched from Servo.
    pub fn enter_hint_mode(&mut self, labels: Vec<String>) {
        self.mode = Mode::Hint { filter: String::new(), labels };
    }

    fn handle_normal(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        let m = &event.modifiers;
        match event.key {
            CoreKey::Char('i') if !m.ctrl => {
                self.mode = Mode::Insert;
                vec![Action::EnterInsert]
            }
            CoreKey::Char(':') if !m.ctrl => {
                self.mode = Mode::Command { buffer: String::new() };
                vec![Action::EnterCommand]
            }
            CoreKey::Char('f') if !m.ctrl => {
                vec![Action::EnterHintMode]
            }
            CoreKey::Char('h') if !m.ctrl && !m.shift => vec![Action::FocusNeighbor(Direction::Left)],
            CoreKey::Char('j') if !m.ctrl && !m.shift => vec![Action::FocusNeighbor(Direction::Down)],
            CoreKey::Char('k') if !m.ctrl && !m.shift => vec![Action::FocusNeighbor(Direction::Up)],
            CoreKey::Char('l') if !m.ctrl && !m.shift => vec![Action::FocusNeighbor(Direction::Right)],
            CoreKey::Char('H') if !m.ctrl => vec![Action::ResizeSplit(Direction::Left, 0.05)],
            CoreKey::Char('J') if !m.ctrl => vec![Action::ResizeSplit(Direction::Down, 0.05)],
            CoreKey::Char('K') if !m.ctrl => vec![Action::ResizeSplit(Direction::Up, 0.05)],
            CoreKey::Char('L') if !m.ctrl => vec![Action::ResizeSplit(Direction::Right, 0.05)],
            CoreKey::Char('v') if m.ctrl => vec![Action::SplitView(SplitDirection::Vertical)],
            CoreKey::Char('s') if m.ctrl => vec![Action::SplitView(SplitDirection::Horizontal)],
            CoreKey::Char('q') if !m.ctrl => vec![Action::CloseView],
            CoreKey::Char('r') if !m.ctrl => vec![Action::Reload],
            CoreKey::Char('b') if m.ctrl => vec![Action::Back],
            CoreKey::Char('f') if m.ctrl => vec![Action::Forward],
            _ => vec![],
        }
    }

    fn handle_insert(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        if event.key == CoreKey::Escape {
            self.mode = Mode::Normal;
            return vec![Action::ExitToNormal];
        }
        vec![Action::ForwardToServo(event.clone())]
    }

    fn handle_command(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        match event.key {
            CoreKey::Escape => {
                self.mode = Mode::Normal;
                vec![Action::ExitToNormal]
            }
            CoreKey::Enter => {
                let buffer = if let Mode::Command { buffer } = &self.mode {
                    buffer.clone()
                } else {
                    unreachable!()
                };
                self.mode = Mode::Normal;
                self.parse_command(&buffer)
            }
            CoreKey::Backspace => {
                if let Mode::Command { buffer } = &mut self.mode {
                    buffer.pop();
                }
                vec![]
            }
            CoreKey::Char(c) => {
                if let Mode::Command { buffer } = &mut self.mode {
                    buffer.push(c);
                }
                vec![]
            }
            _ => vec![],
        }
    }

    fn handle_hint(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        match event.key {
            CoreKey::Escape => {
                self.mode = Mode::Normal;
                vec![Action::ExitToNormal]
            }
            CoreKey::Char(c) => {
                let (new_filter, matching) = if let Mode::Hint { filter, labels } = &self.mode {
                    let mut f = filter.clone();
                    f.push(c);
                    let matching: Vec<String> = labels
                        .iter()
                        .filter(|l| l.starts_with(&f))
                        .cloned()
                        .collect();
                    (f, matching)
                } else {
                    unreachable!()
                };

                if matching.len() == 1 {
                    let label = matching[0].clone();
                    self.mode = Mode::Normal;
                    vec![Action::ActivateHint(label)]
                } else if matching.is_empty() {
                    // No match — cancel hint mode
                    self.mode = Mode::Normal;
                    vec![Action::ExitToNormal]
                } else {
                    if let Mode::Hint { filter, .. } = &mut self.mode {
                        *filter = new_filter;
                    }
                    vec![Action::HintCharTyped(c)]
                }
            }
            _ => vec![],
        }
    }

    fn parse_command(&self, cmd: &str) -> Vec<Action> {
        let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
        match parts.first().copied() {
            Some("open" | "o") => {
                if let Some(url) = parts.get(1) {
                    vec![Action::Navigate(url.to_string())]
                } else {
                    vec![]
                }
            }
            Some("quit" | "q") => vec![Action::Quit],
            Some("save") => vec![Action::SaveSession],
            Some("restore") => vec![Action::RestoreSession],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> CoreKeyEvent {
        CoreKeyEvent {
            key: CoreKey::Char(c),
            state: KeyState::Pressed,
            modifiers: Modifiers::default(),
        }
    }

    fn ctrl_key(c: char) -> CoreKeyEvent {
        CoreKeyEvent {
            key: CoreKey::Char(c),
            state: KeyState::Pressed,
            modifiers: Modifiers { ctrl: true, ..Default::default() },
        }
    }

    fn special(k: CoreKey) -> CoreKeyEvent {
        CoreKeyEvent {
            key: k,
            state: KeyState::Pressed,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn starts_in_normal_mode() {
        let router = InputRouter::new();
        assert_eq!(*router.mode(), Mode::Normal);
    }

    #[test]
    fn i_enters_insert_mode() {
        let mut router = InputRouter::new();
        let actions = router.handle(&key('i'));
        assert_eq!(actions, vec![Action::EnterInsert]);
        assert!(matches!(router.mode(), Mode::Insert));
    }

    #[test]
    fn esc_returns_to_normal_from_insert() {
        let mut router = InputRouter::new();
        router.handle(&key('i'));
        let actions = router.handle(&special(CoreKey::Escape));
        assert_eq!(actions, vec![Action::ExitToNormal]);
        assert_eq!(*router.mode(), Mode::Normal);
    }

    #[test]
    fn insert_forwards_keys_to_servo() {
        let mut router = InputRouter::new();
        router.handle(&key('i'));
        let actions = router.handle(&key('a'));
        assert_eq!(actions, vec![Action::ForwardToServo(key('a'))]);
    }

    #[test]
    fn colon_enters_command_mode() {
        let mut router = InputRouter::new();
        let actions = router.handle(&key(':'));
        assert_eq!(actions, vec![Action::EnterCommand]);
        assert!(matches!(router.mode(), Mode::Command { .. }));
    }

    #[test]
    fn command_mode_builds_buffer() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        router.handle(&key('o'));
        router.handle(&key('p'));
        if let Mode::Command { buffer } = router.mode() {
            assert_eq!(buffer, "op");
        } else {
            panic!("not in command mode");
        }
    }

    #[test]
    fn command_enter_executes_open() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        for c in "open https://example.com".chars() {
            router.handle(&key(c));
        }
        let actions = router.handle(&special(CoreKey::Enter));
        assert_eq!(actions, vec![Action::Navigate("https://example.com".into())]);
        assert_eq!(*router.mode(), Mode::Normal);
    }

    #[test]
    fn command_esc_cancels() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        router.handle(&key('x'));
        let actions = router.handle(&special(CoreKey::Escape));
        assert_eq!(actions, vec![Action::ExitToNormal]);
        assert_eq!(*router.mode(), Mode::Normal);
    }

    #[test]
    fn f_triggers_hint_mode_request() {
        let mut router = InputRouter::new();
        let actions = router.handle(&key('f'));
        assert_eq!(actions, vec![Action::EnterHintMode]);
        // Still in Normal — app will call enter_hint_mode after fetching labels
        assert_eq!(*router.mode(), Mode::Normal);
    }

    #[test]
    fn hint_mode_filters_and_activates() {
        let mut router = InputRouter::new();
        router.enter_hint_mode(vec!["as".into(), "ad".into(), "af".into()]);
        let actions = router.handle(&key('a'));
        assert_eq!(actions, vec![Action::HintCharTyped('a')]);
        // Second char narrows to one
        let actions = router.handle(&key('s'));
        assert_eq!(actions, vec![Action::ActivateHint("as".into())]);
        assert_eq!(*router.mode(), Mode::Normal);
    }

    #[test]
    fn hint_mode_esc_cancels() {
        let mut router = InputRouter::new();
        router.enter_hint_mode(vec!["as".into()]);
        let actions = router.handle(&special(CoreKey::Escape));
        assert_eq!(actions, vec![Action::ExitToNormal]);
        assert_eq!(*router.mode(), Mode::Normal);
    }

    #[test]
    fn normal_hjkl_focus_neighbors() {
        let mut router = InputRouter::new();
        assert_eq!(router.handle(&key('h')), vec![Action::FocusNeighbor(Direction::Left)]);
        assert_eq!(router.handle(&key('j')), vec![Action::FocusNeighbor(Direction::Down)]);
        assert_eq!(router.handle(&key('k')), vec![Action::FocusNeighbor(Direction::Up)]);
        assert_eq!(router.handle(&key('l')), vec![Action::FocusNeighbor(Direction::Right)]);
    }

    #[test]
    fn ctrl_v_splits_vertical() {
        let mut router = InputRouter::new();
        assert_eq!(router.handle(&ctrl_key('v')), vec![Action::SplitView(SplitDirection::Vertical)]);
    }

    #[test]
    fn ctrl_s_splits_horizontal() {
        let mut router = InputRouter::new();
        assert_eq!(router.handle(&ctrl_key('s')), vec![Action::SplitView(SplitDirection::Horizontal)]);
    }

    #[test]
    fn q_closes_view() {
        let mut router = InputRouter::new();
        assert_eq!(router.handle(&key('q')), vec![Action::CloseView]);
    }

    #[test]
    fn released_keys_are_ignored() {
        let mut router = InputRouter::new();
        let event = CoreKeyEvent {
            key: CoreKey::Char('i'),
            state: KeyState::Released,
            modifiers: Modifiers::default(),
        };
        assert!(router.handle(&event).is_empty());
        assert_eq!(*router.mode(), Mode::Normal);
    }

    #[test]
    fn command_backspace_removes_char() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        router.handle(&key('a'));
        router.handle(&key('b'));
        router.handle(&special(CoreKey::Backspace));
        if let Mode::Command { buffer } = router.mode() {
            assert_eq!(buffer, "a");
        } else {
            panic!("not in command mode");
        }
    }
}
