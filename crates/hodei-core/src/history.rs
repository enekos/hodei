use std::rc::Rc;
use rusqlite::{params, Connection};

pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visited_at: String,
}

pub struct HistoryManager {
    conn: Rc<Connection>,
    max_entries: usize,
}

impl HistoryManager {
    pub fn new(conn: Rc<Connection>, max_entries: usize) -> Self {
        Self { conn, max_entries }
    }

    pub fn record_visit(&self, url: &str, title: &str) -> Result<(), rusqlite::Error> {
        let existing: Option<i64> = self.conn
            .query_row("SELECT id FROM history WHERE url = ?1", params![url], |row| row.get(0))
            .ok();

        if let Some(id) = existing {
            self.conn.execute(
                "UPDATE history SET title = ?1, visit_count = visit_count + 1, last_visited_at = datetime('now') WHERE id = ?2",
                params![title, id],
            )?;
        } else {
            self.conn.execute(
                "INSERT INTO history (url, title) VALUES (?1, ?2)",
                params![url, title],
            )?;
            self.prune()?;
        }
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>, rusqlite::Error> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT url, title, visit_count, last_visited_at FROM history \
             WHERE url LIKE ?1 OR title LIKE ?1 \
             ORDER BY visit_count DESC, last_visited_at DESC \
             LIMIT ?2"
        )?;
        let entries = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(HistoryEntry {
                url: row.get(0)?,
                title: row.get(1)?,
                visit_count: row.get(2)?,
                last_visited_at: row.get(3)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<HistoryEntry>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT url, title, visit_count, last_visited_at FROM history \
             ORDER BY last_visited_at DESC LIMIT ?1"
        )?;
        let entries = stmt.query_map(params![limit as i64], |row| {
            Ok(HistoryEntry {
                url: row.get(0)?,
                title: row.get(1)?,
                visit_count: row.get(2)?,
                last_visited_at: row.get(3)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    fn prune(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "DELETE FROM history WHERE id IN (
                SELECT id FROM history ORDER BY last_visited_at DESC LIMIT -1 OFFSET ?1
            )",
            params![self.max_entries as i64],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn make_history() -> HistoryManager {
        let conn = db::open_database_in_memory().unwrap();
        HistoryManager::new(conn, 100)
    }

    #[test]
    fn record_and_search() {
        let hm = make_history();
        hm.record_visit("https://example.com", "Example").unwrap();
        hm.record_visit("https://rust-lang.org", "Rust").unwrap();
        let results = hm.search("rust", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://rust-lang.org");
    }

    #[test]
    fn record_increments_visit_count() {
        let hm = make_history();
        hm.record_visit("https://example.com", "Example").unwrap();
        hm.record_visit("https://example.com", "Example - Updated").unwrap();
        let results = hm.search("example", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].visit_count, 2);
        assert_eq!(results[0].title, "Example - Updated");
    }

    #[test]
    fn recent_returns_by_time() {
        let hm = make_history();
        hm.record_visit("https://a.com", "A").unwrap();
        hm.record_visit("https://b.com", "B").unwrap();
        let recent = hm.recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].url, "https://b.com");
    }

    #[test]
    fn prune_respects_max_entries() {
        let conn = db::open_database_in_memory().unwrap();
        let hm = HistoryManager::new(conn, 3);
        for i in 0..5 {
            hm.record_visit(&format!("https://{}.com", i), &format!("Site {}", i)).unwrap();
        }
        let all = hm.recent(100).unwrap();
        assert!(all.len() <= 3);
    }

    #[test]
    fn search_empty_query_returns_all() {
        let hm = make_history();
        hm.record_visit("https://a.com", "A").unwrap();
        hm.record_visit("https://b.com", "B").unwrap();
        let results = hm.search("", 10).unwrap();
        assert_eq!(results.len(), 2);
    }
}
