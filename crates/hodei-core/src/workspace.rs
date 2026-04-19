use std::collections::HashMap;
use std::rc::Rc;
use rusqlite::Connection;
use crate::session::SessionManager;
use crate::types::*;

pub struct WorkspaceState {
    pub nodes: Vec<LayoutNodeRow>,
    pub tiles: Vec<TileRow>,
    pub focused: Option<ViewId>,
}

pub struct WorkspaceManager {
    session: SessionManager,
    active: String,
    cache: HashMap<String, WorkspaceState>,
}

impl WorkspaceManager {
    pub fn new(conn: Rc<Connection>) -> Self {
        let session = SessionManager::new(conn);
        Self {
            session,
            active: String::new(),
            cache: HashMap::new(),
        }
    }

    pub fn active_name(&self) -> &str {
        &self.active
    }

    pub fn save_active(
        &mut self,
        nodes: &[LayoutNodeRow],
        tiles: &[TileRow],
        focused: Option<ViewId>,
    ) -> Result<(), rusqlite::Error> {
        if self.active.is_empty() {
            return Ok(());
        }
        self.cache.insert(self.active.clone(), WorkspaceState {
            nodes: nodes.to_vec(),
            tiles: tiles.to_vec(),
            focused,
        });
        self.session.save(&self.active, nodes, tiles, focused)
    }

    pub fn switch_to(
        &mut self,
        name: &str,
        current_nodes: &[LayoutNodeRow],
        current_tiles: &[TileRow],
        current_focused: Option<ViewId>,
    ) -> Result<Option<WorkspaceState>, rusqlite::Error> {
        // Save current workspace
        if !self.active.is_empty() {
            self.cache.insert(self.active.clone(), WorkspaceState {
                nodes: current_nodes.to_vec(),
                tiles: current_tiles.to_vec(),
                focused: current_focused,
            });
            self.session.save(&self.active, current_nodes, current_tiles, current_focused)?;
        }

        self.active = name.to_string();

        // Try cache first
        if let Some(state) = self.cache.get(name) {
            return Ok(Some(WorkspaceState {
                nodes: state.nodes.clone(),
                tiles: state.tiles.clone(),
                focused: state.focused,
            }));
        }

        // Load from DB
        match self.session.restore(name)? {
            Some((nodes, tiles, focused)) => {
                let state = WorkspaceState {
                    nodes: nodes.clone(),
                    tiles: tiles.clone(),
                    focused,
                };
                self.cache.insert(name.to_string(), WorkspaceState { nodes, tiles, focused });
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }

    pub fn create_new(&mut self, name: &str) {
        self.active = name.to_string();
        self.cache.insert(name.to_string(), WorkspaceState {
            nodes: vec![],
            tiles: vec![],
            focused: None,
        });
    }

    pub fn delete(&mut self, name: &str) -> Result<bool, rusqlite::Error> {
        if name == self.active {
            return Ok(false);
        }
        self.cache.remove(name);
        self.session.delete(name)?;
        Ok(true)
    }

    pub fn list(&self) -> Result<Vec<SessionInfo>, rusqlite::Error> {
        self.session.list()
    }

    pub fn set_active(&mut self, name: &str) {
        self.active = name.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn make_workspace_manager() -> WorkspaceManager {
        let conn = db::open_database_in_memory().unwrap();
        WorkspaceManager::new(conn)
    }

    fn sample_state() -> (Vec<LayoutNodeRow>, Vec<TileRow>, Option<ViewId>) {
        let nodes = vec![LayoutNodeRow {
            node_index: 0,
            is_leaf: true,
            direction: None,
            ratio: None,
            view_id: Some(ViewId(1)),
        }];
        let tiles = vec![TileRow {
            view_id: ViewId(1),
            url: "https://example.com".into(),
            title: "Ex".into(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        }];
        (nodes, tiles, Some(ViewId(1)))
    }

    #[test]
    fn save_and_switch() {
        let mut wm = make_workspace_manager();
        let (nodes, tiles, focused) = sample_state();
        wm.set_active("work");
        wm.save_active(&nodes, &tiles, focused).unwrap();

        let result = wm.switch_to("play", &nodes, &tiles, focused).unwrap();
        assert!(result.is_none());
        assert_eq!(wm.active_name(), "play");

        let result = wm.switch_to("work", &[], &[], None).unwrap();
        assert!(result.is_some());
        let state = result.unwrap();
        assert_eq!(state.tiles[0].url, "https://example.com");
    }

    #[test]
    fn delete_non_active_succeeds() {
        let mut wm = make_workspace_manager();
        let (nodes, tiles, focused) = sample_state();
        wm.set_active("keep");
        wm.save_active(&nodes, &tiles, focused).unwrap();
        wm.set_active("other");
        assert!(wm.delete("keep").unwrap());
    }

    #[test]
    fn delete_active_fails() {
        let mut wm = make_workspace_manager();
        wm.set_active("current");
        assert!(!wm.delete("current").unwrap());
    }

    #[test]
    fn cache_avoids_db_roundtrip() {
        let mut wm = make_workspace_manager();
        let (nodes, tiles, focused) = sample_state();
        wm.set_active("cached");
        wm.save_active(&nodes, &tiles, focused).unwrap();

        wm.set_active("other");
        let result = wm.switch_to("cached", &[], &[], None).unwrap();
        assert!(result.is_some());
    }
}
