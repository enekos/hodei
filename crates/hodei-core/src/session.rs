use std::rc::Rc;
use rusqlite::{params, Connection};
use crate::types::*;

pub struct SessionManager {
    conn: Rc<Connection>,
}

impl SessionManager {
    pub fn new(conn: Rc<Connection>) -> Self {
        Self { conn }
    }

    pub fn save(
        &self,
        name: &str,
        nodes: &[LayoutNodeRow],
        tiles: &[TileRow],
        focused: Option<ViewId>,
    ) -> Result<(), rusqlite::Error> {
        let tx = self.conn.unchecked_transaction()?;

        // Delete existing session with this name
        tx.execute("DELETE FROM sessions WHERE name = ?1", params![name])?;

        // Create new session
        tx.execute(
            "INSERT INTO sessions (name) VALUES (?1)",
            params![name],
        )?;
        let session_id = tx.last_insert_rowid();

        // Insert tiles
        for tile in tiles {
            tx.execute(
                "INSERT INTO tiles (session_id, view_id, url, title, scroll_x, scroll_y) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![session_id, tile.view_id.0 as i64, tile.url, tile.title, tile.scroll_x, tile.scroll_y],
            )?;
        }

        // Insert layout nodes
        for node in nodes {
            let dir_str = node.direction.map(|d| match d {
                SplitDirection::Horizontal => "h",
                SplitDirection::Vertical => "v",
            });
            tx.execute(
                "INSERT INTO layout_tree (session_id, node_index, is_leaf, direction, ratio, view_id, focused_view_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    session_id,
                    node.node_index as i64,
                    node.is_leaf as i32,
                    dir_str,
                    node.ratio,
                    node.view_id.map(|v| v.0 as i64),
                    focused.map(|v| v.0 as i64),
                ],
            )?;
        }

        tx.commit()
    }

    pub fn restore(&self, name: &str) -> Result<Option<(Vec<LayoutNodeRow>, Vec<TileRow>, Option<ViewId>)>, rusqlite::Error> {
        let session_id: Option<i64> = self.conn
            .query_row(
                "SELECT id FROM sessions WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .ok();

        let session_id = match session_id {
            Some(id) => id,
            None => return Ok(None),
        };

        // Read tiles
        let mut stmt = self.conn.prepare(
            "SELECT view_id, url, title, scroll_x, scroll_y FROM tiles WHERE session_id = ?1"
        )?;
        let tiles: Vec<TileRow> = stmt.query_map(params![session_id], |row| {
            Ok(TileRow {
                view_id: ViewId(row.get::<_, i64>(0)? as u64),
                url: row.get(1)?,
                title: row.get(2)?,
                scroll_x: row.get(3)?,
                scroll_y: row.get(4)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        // Read layout nodes
        let mut stmt = self.conn.prepare(
            "SELECT node_index, is_leaf, direction, ratio, view_id, focused_view_id FROM layout_tree WHERE session_id = ?1 ORDER BY node_index"
        )?;
        let mut focused: Option<ViewId> = None;
        let nodes: Vec<LayoutNodeRow> = stmt.query_map(params![session_id], |row| {
            let fv: Option<i64> = row.get(5)?;
            if let Some(fv) = fv {
                focused = Some(ViewId(fv as u64));
            }
            let dir_str: Option<String> = row.get(2)?;
            Ok(LayoutNodeRow {
                node_index: row.get::<_, i64>(0)? as u32,
                is_leaf: row.get::<_, i32>(1)? != 0,
                direction: dir_str.map(|s| match s.as_str() {
                    "h" => SplitDirection::Horizontal,
                    "v" => SplitDirection::Vertical,
                    _ => SplitDirection::Vertical,
                }),
                ratio: row.get(3)?,
                view_id: row.get::<_, Option<i64>>(4)?.map(|v| ViewId(v as u64)),
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(Some((nodes, tiles, focused)))
    }

    pub fn autosave(
        &self,
        nodes: &[LayoutNodeRow],
        tiles: &[TileRow],
        focused: Option<ViewId>,
    ) -> Result<(), rusqlite::Error> {
        self.save("default", nodes, tiles, focused)
    }

    pub fn list(&self) -> Result<Vec<SessionInfo>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, s.created_at, s.updated_at, COUNT(t.id) \
             FROM sessions s LEFT JOIN tiles t ON t.session_id = s.id \
             GROUP BY s.id ORDER BY s.updated_at DESC"
        )?;
        let sessions = stmt.query_map([], |row| {
            Ok(SessionInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                tile_count: row.get::<_, i64>(4)? as usize,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(sessions)
    }

    pub fn delete(&self, name: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute("DELETE FROM sessions WHERE name = ?1", params![name])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn make_session_manager() -> SessionManager {
        let conn = db::open_database_in_memory().unwrap();
        SessionManager::new(conn)
    }

    fn sample_tiles() -> Vec<TileRow> {
        vec![
            TileRow {
                view_id: ViewId(1),
                url: "https://example.com".into(),
                title: "Example".into(),
                scroll_x: 0.0,
                scroll_y: 100.0,
            },
            TileRow {
                view_id: ViewId(2),
                url: "https://rust-lang.org".into(),
                title: "Rust".into(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        ]
    }

    fn sample_nodes() -> Vec<LayoutNodeRow> {
        vec![
            LayoutNodeRow {
                node_index: 0,
                is_leaf: false,
                direction: Some(SplitDirection::Vertical),
                ratio: Some(0.5),
                view_id: None,
            },
            LayoutNodeRow {
                node_index: 1,
                is_leaf: true,
                direction: None,
                ratio: None,
                view_id: Some(ViewId(1)),
            },
            LayoutNodeRow {
                node_index: 2,
                is_leaf: true,
                direction: None,
                ratio: None,
                view_id: Some(ViewId(2)),
            },
        ]
    }

    #[test]
    fn open_creates_tables() {
        let sm = make_session_manager();
        // Should not panic — tables exist
        let count: i64 = sm.conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn save_and_restore_roundtrip() {
        let sm = make_session_manager();
        let tiles = sample_tiles();
        let nodes = sample_nodes();
        sm.save("test", &nodes, &tiles, Some(ViewId(1))).unwrap();

        let (restored_nodes, restored_tiles, focused) = sm.restore("test").unwrap().unwrap();
        assert_eq!(restored_nodes.len(), 3);
        assert_eq!(restored_tiles.len(), 2);
        assert_eq!(focused, Some(ViewId(1)));
        assert_eq!(restored_tiles[0].url, "https://example.com");
        assert_eq!(restored_nodes[0].direction, Some(SplitDirection::Vertical));
    }

    #[test]
    fn restore_nonexistent_returns_none() {
        let sm = make_session_manager();
        assert!(sm.restore("nope").unwrap().is_none());
    }

    #[test]
    fn autosave_overwrites_default() {
        let sm = make_session_manager();
        let tiles1 = vec![TileRow {
            view_id: ViewId(1),
            url: "https://a.com".into(),
            title: "A".into(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        }];
        let nodes1 = vec![LayoutNodeRow {
            node_index: 0,
            is_leaf: true,
            direction: None,
            ratio: None,
            view_id: Some(ViewId(1)),
        }];
        sm.autosave(&nodes1, &tiles1, Some(ViewId(1))).unwrap();

        // Autosave again with different data
        let tiles2 = vec![TileRow {
            view_id: ViewId(5),
            url: "https://b.com".into(),
            title: "B".into(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        }];
        sm.autosave(&nodes1, &tiles2, Some(ViewId(5))).unwrap();

        let (_, restored_tiles, focused) = sm.restore("default").unwrap().unwrap();
        assert_eq!(restored_tiles.len(), 1);
        assert_eq!(restored_tiles[0].url, "https://b.com");
        assert_eq!(focused, Some(ViewId(5)));
    }

    #[test]
    fn list_sessions() {
        let sm = make_session_manager();
        sm.save("alpha", &sample_nodes(), &sample_tiles(), None).unwrap();
        sm.save("beta", &[], &[], None).unwrap();
        let list = sm.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn delete_session() {
        let sm = make_session_manager();
        sm.save("doomed", &sample_nodes(), &sample_tiles(), None).unwrap();
        sm.delete("doomed").unwrap();
        assert!(sm.restore("doomed").unwrap().is_none());
    }
}
