use crate::types::*;
use std::collections::HashMap;

// === KeyCombo ===

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: CoreKey,
    pub ctrl: bool,
    pub shift: bool,
}

impl KeyCombo {
    pub fn new(key: CoreKey, ctrl: bool, shift: bool) -> Self {
        Self { key, ctrl, shift }
    }

    pub fn from_event(event: &CoreKeyEvent) -> Self {
        let shift = match event.key {
            // If key is already uppercase, shift is implicit — don't include it in the combo
            CoreKey::Char(c) if c.is_uppercase() => false,
            _ => event.modifiers.shift,
        };
        Self {
            key: event.key,
            ctrl: event.modifiers.ctrl,
            shift,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('+').collect();
        let mut ctrl = false;
        let mut shift = false;

        for &part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "ctrl" => ctrl = true,
                "shift" => shift = true,
                _ => return None,
            }
        }

        let key_str = parts.last()?;
        let key = match *key_str {
            "escape" | "esc" => CoreKey::Escape,
            "enter" => CoreKey::Enter,
            "backspace" => CoreKey::Backspace,
            "tab" => CoreKey::Tab,
            "left" => CoreKey::Left,
            "right" => CoreKey::Right,
            "up" => CoreKey::Up,
            "down" => CoreKey::Down,
            "home" => CoreKey::Home,
            "end" => CoreKey::End,
            "pageup" => CoreKey::PageUp,
            "pagedown" => CoreKey::PageDown,
            s if s.len() == 1 => {
                let ch = s.chars().next()?;
                if ch.is_uppercase() {
                    shift = true;
                }
                CoreKey::Char(ch)
            }
            _ => return None,
        };

        Some(Self { key, ctrl, shift })
    }
}

// === Modes ===

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Command { buffer: String },
    Hint { filter: String, labels: Vec<String> },
    Search { query: String },
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
    Bookmark(Option<String>),
    BookmarkDelete(String),
    ShowBookmarks(String),
    ShowHistory(String),
    SuggestionNext,
    SuggestionPrev,
    CommandBufferChanged,
    EnterSearch,
    SearchQueryChanged(String),
    SearchNext,
    SearchPrev,
    SearchClear,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    YankUrl,
    YankTitle,
    WorkspaceSwitch(String),
    WorkspaceNew(String),
    WorkspaceDelete(String),
    WorkspaceList,
    FocusNext,
    FocusPrev,
    ScrollPageDown,
    ScrollPageUp,
    ScrollToTop,
    ScrollToBottom,
    HardReload,
    PasteNavigate,
    PasteNewTile,
    DuplicateTile,
    ViewSource,
    GoHome,
    GoUp,
    GoToRoot,
    ResetSplits,
    SwapTiles,
    SetQuickmark(u8),
    JumpQuickmark(u8),
    ToggleTheme,
    ShowShortcuts,
    DevToolsShow,
}

// === Router ===

pub struct InputRouter {
    mode: Mode,
    normal_bindings: HashMap<KeyCombo, Action>,
}

impl Default for InputRouter {
    fn default() -> Self { Self::new() }
}

impl InputRouter {
    pub fn new() -> Self {
        log::debug!("InputRouter::new: initializing with default bindings");
        Self {
            mode: Mode::Normal,
            normal_bindings: Self::default_bindings(),
        }
    }

    pub fn with_overrides(overrides: &HashMap<String, String>) -> Self {
        log::info!("InputRouter::with_overrides: applying {} custom bindings", overrides.len());
        let mut bindings = Self::default_bindings();
        for (action_name, key_str) in overrides {
            if let Some(combo) = KeyCombo::parse(key_str) {
                if let Some(action) = Self::action_from_name(action_name) {
                    log::debug!("InputRouter::with_overrides: binding '{}' -> {:?}", key_str, action);
                    bindings.retain(|_, v| v != &action);
                    bindings.insert(combo, action);
                } else {
                    log::warn!("InputRouter::with_overrides: unknown action '{}'", action_name);
                }
            } else {
                log::warn!("InputRouter::with_overrides: failed to parse key combo '{}'", key_str);
            }
        }
        Self {
            mode: Mode::Normal,
            normal_bindings: bindings,
        }
    }

    fn default_bindings() -> HashMap<KeyCombo, Action> {
        let mut map = HashMap::new();
        map.insert(KeyCombo::new(CoreKey::Char('i'), false, false), Action::EnterInsert);
        map.insert(KeyCombo::new(CoreKey::Char(':'), false, false), Action::EnterCommand);
        map.insert(KeyCombo::new(CoreKey::Char('f'), false, false), Action::EnterHintMode);
        map.insert(KeyCombo::new(CoreKey::Char('/'), false, false), Action::EnterSearch);
        map.insert(KeyCombo::new(CoreKey::Char('h'), false, false), Action::FocusNeighbor(Direction::Left));
        map.insert(KeyCombo::new(CoreKey::Char('j'), false, false), Action::FocusNeighbor(Direction::Down));
        map.insert(KeyCombo::new(CoreKey::Char('k'), false, false), Action::FocusNeighbor(Direction::Up));
        map.insert(KeyCombo::new(CoreKey::Char('l'), false, false), Action::FocusNeighbor(Direction::Right));
        map.insert(KeyCombo::new(CoreKey::Char('H'), false, false), Action::ResizeSplit(Direction::Left, 0.05));
        map.insert(KeyCombo::new(CoreKey::Char('J'), false, false), Action::ResizeSplit(Direction::Down, 0.05));
        map.insert(KeyCombo::new(CoreKey::Char('K'), false, false), Action::ResizeSplit(Direction::Up, 0.05));
        map.insert(KeyCombo::new(CoreKey::Char('L'), false, false), Action::ResizeSplit(Direction::Right, 0.05));
        map.insert(KeyCombo::new(CoreKey::Char('v'), true, false), Action::SplitView(SplitDirection::Vertical));
        map.insert(KeyCombo::new(CoreKey::Char('s'), true, false), Action::SplitView(SplitDirection::Horizontal));
        map.insert(KeyCombo::new(CoreKey::Char('q'), false, false), Action::CloseView);
        map.insert(KeyCombo::new(CoreKey::Char('r'), false, false), Action::Reload);
        map.insert(KeyCombo::new(CoreKey::Char('b'), true, false), Action::Back);
        map.insert(KeyCombo::new(CoreKey::Char('f'), true, false), Action::Forward);
        map.insert(KeyCombo::new(CoreKey::Char('n'), false, false), Action::SearchNext);
        map.insert(KeyCombo::new(CoreKey::Char('N'), false, false), Action::SearchPrev);
        map.insert(KeyCombo::new(CoreKey::Char('+'), false, false), Action::ZoomIn);
        map.insert(KeyCombo::new(CoreKey::Char('-'), false, false), Action::ZoomOut);
        map.insert(KeyCombo::new(CoreKey::Char('0'), false, false), Action::ZoomReset);
        map.insert(KeyCombo::new(CoreKey::Char('y'), false, false), Action::YankUrl);
        map.insert(KeyCombo::new(CoreKey::Char('Y'), false, false), Action::YankTitle);
        map.insert(KeyCombo::new(CoreKey::Char('B'), false, false), Action::ShowBookmarks(String::new()));
        map.insert(KeyCombo::new(CoreKey::Tab, true, false), Action::FocusNext);
        map.insert(KeyCombo::new(CoreKey::Tab, true, true), Action::FocusPrev);
        map.insert(KeyCombo::new(CoreKey::Char('d'), true, false), Action::ScrollPageDown);
        map.insert(KeyCombo::new(CoreKey::Char('u'), true, false), Action::ScrollPageUp);
        map.insert(KeyCombo::new(CoreKey::Char('R'), false, false), Action::HardReload);
        map.insert(KeyCombo::new(CoreKey::Char('p'), false, false), Action::PasteNavigate);
        map.insert(KeyCombo::new(CoreKey::Char('P'), false, false), Action::PasteNewTile);
        map.insert(KeyCombo::new(CoreKey::Char('D'), false, false), Action::DuplicateTile);
        map.insert(KeyCombo::new(CoreKey::Char('F'), false, false), Action::ViewSource);
        map.insert(KeyCombo::new(CoreKey::Char('G'), false, false), Action::GoHome);
        map.insert(KeyCombo::new(CoreKey::Home, false, false), Action::ScrollToTop);
        map.insert(KeyCombo::new(CoreKey::End, false, false), Action::ScrollToBottom);
        map.insert(KeyCombo::new(CoreKey::PageUp, false, false), Action::ScrollPageUp);
        map.insert(KeyCombo::new(CoreKey::PageDown, false, false), Action::ScrollPageDown);
        map.insert(KeyCombo::new(CoreKey::Char('='), false, false), Action::ResetSplits);
        map.insert(KeyCombo::new(CoreKey::Char('z'), false, false), Action::SwapTiles);
        map.insert(KeyCombo::new(CoreKey::Char('t'), false, false), Action::ToggleTheme);
        map.insert(KeyCombo::new(CoreKey::Char('?'), false, false), Action::ShowShortcuts);
        for i in 0..=9u8 {
            let c = (b'0' + i) as char;
            map.insert(KeyCombo::new(CoreKey::Char(c), true, false), Action::JumpQuickmark(i));
            map.insert(KeyCombo::new(CoreKey::Char(c), false, true), Action::SetQuickmark(i));
        }
        log::debug!("InputRouter::default_bindings: {} bindings registered", map.len());
        map
    }

    fn action_from_name(name: &str) -> Option<Action> {
        match name {
            "focus_left" => Some(Action::FocusNeighbor(Direction::Left)),
            "focus_down" => Some(Action::FocusNeighbor(Direction::Down)),
            "focus_up" => Some(Action::FocusNeighbor(Direction::Up)),
            "focus_right" => Some(Action::FocusNeighbor(Direction::Right)),
            "focus_next" => Some(Action::FocusNext),
            "focus_prev" => Some(Action::FocusPrev),
            "split_vertical" => Some(Action::SplitView(SplitDirection::Vertical)),
            "split_horizontal" => Some(Action::SplitView(SplitDirection::Horizontal)),
            "close" => Some(Action::CloseView),
            "reload" => Some(Action::Reload),
            "hard_reload" => Some(Action::HardReload),
            "back" => Some(Action::Back),
            "forward" => Some(Action::Forward),
            "insert" => Some(Action::EnterInsert),
            "command" => Some(Action::EnterCommand),
            "hints" => Some(Action::EnterHintMode),
            "search" => Some(Action::EnterSearch),
            "search_next" => Some(Action::SearchNext),
            "search_prev" => Some(Action::SearchPrev),
            "zoom_in" => Some(Action::ZoomIn),
            "zoom_out" => Some(Action::ZoomOut),
            "zoom_reset" => Some(Action::ZoomReset),
            "yank_url" => Some(Action::YankUrl),
            "yank_title" => Some(Action::YankTitle),
            "bookmarks" => Some(Action::ShowBookmarks(String::new())),
            "scroll_page_down" => Some(Action::ScrollPageDown),
            "scroll_page_up" => Some(Action::ScrollPageUp),
            "paste_navigate" => Some(Action::PasteNavigate),
            "paste_new_tile" => Some(Action::PasteNewTile),
            "duplicate_tile" => Some(Action::DuplicateTile),
            "view_source" => Some(Action::ViewSource),
            "go_home" => Some(Action::GoHome),
            "reset_splits" => Some(Action::ResetSplits),
            "swap_tiles" => Some(Action::SwapTiles),
            "toggle_theme" => Some(Action::ToggleTheme),
            "shortcuts" => Some(Action::ShowShortcuts),
            _ => None,
        }
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn handle(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        if event.state != KeyState::Pressed {
            log::trace!("InputRouter::handle: ignoring released key {:?}", event.key);
            return vec![];
        }
        let actions = match &self.mode {
            Mode::Normal => self.handle_normal(event),
            Mode::Insert => self.handle_insert(event),
            Mode::Command { .. } => self.handle_command(event),
            Mode::Hint { .. } => self.handle_hint(event),
            Mode::Search { .. } => self.handle_search(event),
        };
        log::debug!(
            "InputRouter::handle: mode={:?} key={:?} -> {} action(s)",
            self.mode,
            event.key,
            actions.len()
        );
        actions
    }

    /// Called by app after hint elements are fetched from Servo.
    pub fn enter_hint_mode(&mut self, labels: Vec<String>) {
        log::info!("InputRouter::enter_hint_mode: {} labels", labels.len());
        self.mode = Mode::Hint { filter: String::new(), labels };
    }

    fn handle_normal(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        let combo = KeyCombo::from_event(event);
        if let Some(action) = self.normal_bindings.get(&combo) {
            let action = action.clone();
            log::trace!("InputRouter::handle_normal: combo={:?} -> action={:?}", combo, action);
            match &action {
                Action::EnterInsert => self.mode = Mode::Insert,
                Action::EnterCommand => self.mode = Mode::Command { buffer: String::new() },
                Action::EnterSearch => self.mode = Mode::Search { query: String::new() },
                _ => {}
            }
            vec![action]
        } else {
            log::trace!("InputRouter::handle_normal: combo={:?} -> no binding", combo);
            vec![]
        }
    }

    fn handle_insert(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        if event.key == CoreKey::Escape {
            log::debug!("InputRouter::handle_insert: Escape -> ExitToNormal");
            self.mode = Mode::Normal;
            return vec![Action::ExitToNormal];
        }
        log::trace!("InputRouter::handle_insert: forwarding {:?} to servo", event.key);
        vec![Action::ForwardToServo(event.clone())]
    }

    fn handle_command(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        match event.key {
            CoreKey::Escape => {
                log::debug!("InputRouter::handle_command: Escape -> ExitToNormal");
                self.mode = Mode::Normal;
                vec![Action::ExitToNormal]
            }
            CoreKey::Enter => {
                let buffer = if let Mode::Command { buffer } = &self.mode {
                    buffer.clone()
                } else {
                    String::new()
                };
                log::debug!("InputRouter::handle_command: Enter -> execute '{}'", buffer);
                self.mode = Mode::Normal;
                self.parse_command(&buffer)
            }
            CoreKey::Backspace => {
                if let Mode::Command { buffer } = &mut self.mode {
                    buffer.pop();
                    log::trace!("InputRouter::handle_command: Backspace -> buffer='{}'", buffer);
                }
                vec![Action::CommandBufferChanged]
            }
            CoreKey::Char('n') if event.modifiers.ctrl => {
                log::trace!("InputRouter::handle_command: Ctrl+n -> SuggestionNext");
                vec![Action::SuggestionNext]
            }
            CoreKey::Char('p') if event.modifiers.ctrl => {
                log::trace!("InputRouter::handle_command: Ctrl+p -> SuggestionPrev");
                vec![Action::SuggestionPrev]
            }
            CoreKey::Down => {
                log::trace!("InputRouter::handle_command: Down -> SuggestionNext");
                vec![Action::SuggestionNext]
            }
            CoreKey::Up => {
                log::trace!("InputRouter::handle_command: Up -> SuggestionPrev");
                vec![Action::SuggestionPrev]
            }
            CoreKey::Char(c) => {
                if let Mode::Command { buffer } = &mut self.mode {
                    buffer.push(c);
                    log::trace!("InputRouter::handle_command: Char('{}') -> buffer='{}'", c, buffer);
                }
                vec![Action::CommandBufferChanged]
            }
            _ => vec![],
        }
    }

    fn handle_hint(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        match event.key {
            CoreKey::Escape => {
                log::debug!("InputRouter::handle_hint: Escape -> ExitToNormal");
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
                    (String::new(), vec![])
                };

                log::trace!("InputRouter::handle_hint: Char('{}') filter='{}' matches={}", c, new_filter, matching.len());

                if matching.len() == 1 {
                    let label = matching[0].clone();
                    log::debug!("InputRouter::handle_hint: activated hint '{}'", label);
                    self.mode = Mode::Normal;
                    vec![Action::ActivateHint(label)]
                } else if matching.is_empty() {
                    // No match — cancel hint mode
                    log::debug!("InputRouter::handle_hint: no matches -> cancel");
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

    fn handle_search(&mut self, event: &CoreKeyEvent) -> Vec<Action> {
        match event.key {
            CoreKey::Escape => {
                log::debug!("InputRouter::handle_search: Escape -> SearchClear");
                self.mode = Mode::Normal;
                vec![Action::SearchClear]
            }
            CoreKey::Enter => {
                let query = if let Mode::Search { query } = &self.mode {
                    query.clone()
                } else {
                    String::new()
                };
                log::debug!("InputRouter::handle_search: Enter -> query='{}'", query);
                self.mode = Mode::Normal;
                vec![Action::SearchQueryChanged(query)]
            }
            CoreKey::Backspace => {
                if let Mode::Search { query } = &mut self.mode {
                    query.pop();
                    let q = query.clone();
                    log::trace!("InputRouter::handle_search: Backspace -> query='{}'", q);
                    return vec![Action::SearchQueryChanged(q)];
                }
                vec![]
            }
            CoreKey::Char(c) => {
                if let Mode::Search { query } = &mut self.mode {
                    query.push(c);
                    let q = query.clone();
                    log::trace!("InputRouter::handle_search: Char('{}') -> query='{}'", c, q);
                    return vec![Action::SearchQueryChanged(q)];
                }
                vec![]
            }
            _ => vec![],
        }
    }

    fn parse_command(&self, cmd: &str) -> Vec<Action> {
        let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
        let action = match parts.first().copied() {
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
            Some("bookmark") => {
                let tags = parts.get(1).map(|s| s.to_string());
                vec![Action::Bookmark(tags)]
            }
            Some("bookmark-delete") => {
                if let Some(url) = parts.get(1) {
                    vec![Action::BookmarkDelete(url.to_string())]
                } else {
                    vec![]
                }
            }
            Some("bookmarks") => {
                let query = parts.get(1).unwrap_or(&"").to_string();
                vec![Action::ShowBookmarks(query)]
            }
            Some("history") => {
                let query = parts.get(1).unwrap_or(&"").to_string();
                vec![Action::ShowHistory(query)]
            }
            Some("up") => vec![Action::GoUp],
            Some("root") => vec![Action::GoToRoot],
            Some("workspace" | "ws") => {
                if let Some(name) = parts.get(1) {
                    vec![Action::WorkspaceSwitch(name.to_string())]
                } else {
                    vec![Action::WorkspaceList]
                }
            }
            Some("workspace-new") => {
                if let Some(name) = parts.get(1) {
                    vec![Action::WorkspaceNew(name.to_string())]
                } else {
                    vec![]
                }
            }
            Some("workspace-delete") => {
                if let Some(name) = parts.get(1) {
                    vec![Action::WorkspaceDelete(name.to_string())]
                } else {
                    vec![]
                }
            }
            Some("devtools") => vec![Action::DevToolsShow],
            _ => vec![],
        };
        log::debug!("InputRouter::parse_command: '{}' -> {:?}", cmd, action);
        action
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
    fn command_bookmark() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        for c in "bookmark rust,dev".chars() {
            router.handle(&key(c));
        }
        let actions = router.handle(&special(CoreKey::Enter));
        assert_eq!(actions, vec![Action::Bookmark(Some("rust,dev".into()))]);
    }

    #[test]
    fn command_bookmarks_search() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        for c in "bookmarks rust".chars() {
            router.handle(&key(c));
        }
        let actions = router.handle(&special(CoreKey::Enter));
        assert_eq!(actions, vec![Action::ShowBookmarks("rust".into())]);
    }

    #[test]
    fn command_history_search() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        for c in "history github".chars() {
            router.handle(&key(c));
        }
        let actions = router.handle(&special(CoreKey::Enter));
        assert_eq!(actions, vec![Action::ShowHistory("github".into())]);
    }

    #[test]
    fn shift_b_shows_bookmarks() {
        let mut router = InputRouter::new();
        let event = CoreKeyEvent {
            key: CoreKey::Char('B'),
            state: KeyState::Pressed,
            modifiers: Modifiers { shift: true, ..Default::default() },
        };
        let actions = router.handle(&event);
        assert_eq!(actions, vec![Action::ShowBookmarks(String::new())]);
    }

    #[test]
    fn command_mode_backspace_emits_buffer_changed() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        router.handle(&key('a'));
        let actions = router.handle(&special(CoreKey::Backspace));
        assert_eq!(actions, vec![Action::CommandBufferChanged]);
    }

    #[test]
    fn command_mode_typing_emits_buffer_changed() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        let actions = router.handle(&key('o'));
        assert_eq!(actions, vec![Action::CommandBufferChanged]);
    }

    #[test]
    fn command_mode_arrow_down_emits_suggestion_next() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        let actions = router.handle(&special(CoreKey::Down));
        assert_eq!(actions, vec![Action::SuggestionNext]);
    }

    #[test]
    fn command_mode_ctrl_p_emits_suggestion_prev() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        let actions = router.handle(&ctrl_key('p'));
        assert_eq!(actions, vec![Action::SuggestionPrev]);
    }

    #[test]
    fn command_backspace_removes_char() {
        let mut router = InputRouter::new();
        router.handle(&key(':'));
        router.handle(&key('a'));
        router.handle(&key('b'));
        let actions = router.handle(&special(CoreKey::Backspace));
        assert_eq!(actions, vec![Action::CommandBufferChanged]);
        if let Mode::Command { buffer } = router.mode() {
            assert_eq!(buffer, "a");
        } else {
            panic!("not in command mode");
        }
    }

    #[test]
    fn slash_enters_search_mode() {
        let mut router = InputRouter::new();
        let actions = router.handle(&key('/'));
        assert_eq!(actions, vec![Action::EnterSearch]);
        assert!(matches!(router.mode(), Mode::Search { .. }));
    }

    #[test]
    fn search_mode_builds_query() {
        let mut router = InputRouter::new();
        router.handle(&key('/'));
        let actions = router.handle(&key('r'));
        assert_eq!(actions, vec![Action::SearchQueryChanged("r".into())]);
        let actions = router.handle(&key('u'));
        assert_eq!(actions, vec![Action::SearchQueryChanged("ru".into())]);
    }

    #[test]
    fn search_esc_clears() {
        let mut router = InputRouter::new();
        router.handle(&key('/'));
        router.handle(&key('a'));
        let actions = router.handle(&special(CoreKey::Escape));
        assert_eq!(actions, vec![Action::SearchClear]);
        assert_eq!(*router.mode(), Mode::Normal);
    }

    #[test]
    fn n_triggers_search_next_in_normal() {
        let mut router = InputRouter::new();
        let actions = router.handle(&key('n'));
        assert_eq!(actions, vec![Action::SearchNext]);
    }

    #[test]
    fn shift_n_triggers_search_prev_in_normal() {
        let mut router = InputRouter::new();
        let event = CoreKeyEvent {
            key: CoreKey::Char('N'),
            state: KeyState::Pressed,
            modifiers: Modifiers { shift: true, ..Default::default() },
        };
        let actions = router.handle(&event);
        assert_eq!(actions, vec![Action::SearchPrev]);
    }

    #[test]
    fn custom_keybinding_overrides_default() {
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("focus_left".to_string(), "a".to_string());
        let mut router = InputRouter::with_overrides(&overrides);
        let actions = router.handle(&key('a'));
        assert_eq!(actions, vec![Action::FocusNeighbor(Direction::Left)]);
        let actions = router.handle(&key('h'));
        assert!(actions.is_empty());
    }

    #[test]
    fn keycombo_parse_simple() {
        let combo = KeyCombo::parse("h").unwrap();
        assert_eq!(combo.key, CoreKey::Char('h'));
        assert!(!combo.ctrl);
    }

    #[test]
    fn keycombo_parse_ctrl() {
        let combo = KeyCombo::parse("ctrl+v").unwrap();
        assert_eq!(combo.key, CoreKey::Char('v'));
        assert!(combo.ctrl);
    }

    #[test]
    fn question_mark_shows_shortcuts() {
        let mut router = InputRouter::new();
        let actions = router.handle(&key('?'));
        assert_eq!(actions, vec![Action::ShowShortcuts]);
    }
}
