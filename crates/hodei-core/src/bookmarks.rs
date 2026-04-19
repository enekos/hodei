use std::rc::Rc;
use rusqlite::{params, Connection};

pub struct Bookmark {
    pub url: String,
    pub title: String,
    pub tags: String,
    pub created_at: String,
}

pub struct BookmarkManager {
    conn: Rc<Connection>,
}

impl BookmarkManager {
    pub fn new(conn: Rc<Connection>) -> Self {
        Self { conn }
    }

    pub fn add(&self, url: &str, title: &str, tags: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT OR REPLACE INTO bookmarks (url, title, tags) VALUES (?1, ?2, ?3)",
            params![url, title, tags],
        )?;
        Ok(())
    }

    pub fn remove(&self, url: &str) -> Result<bool, rusqlite::Error> {
        let count = self.conn.execute("DELETE FROM bookmarks WHERE url = ?1", params![url])?;
        Ok(count > 0)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Bookmark>, rusqlite::Error> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT url, title, tags, created_at FROM bookmarks \
             WHERE url LIKE ?1 OR title LIKE ?1 OR tags LIKE ?1 \
             ORDER BY created_at DESC, id DESC LIMIT ?2"
        )?;
        let entries = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(Bookmark {
                url: row.get(0)?,
                title: row.get(1)?,
                tags: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    pub fn list_all(&self, limit: usize) -> Result<Vec<Bookmark>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT url, title, tags, created_at FROM bookmarks ORDER BY created_at DESC, id DESC LIMIT ?1"
        )?;
        let entries = stmt.query_map(params![limit as i64], |row| {
            Ok(Bookmark {
                url: row.get(0)?,
                title: row.get(1)?,
                tags: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    pub fn is_bookmarked(&self, url: &str) -> Result<bool, rusqlite::Error> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM bookmarks WHERE url = ?1",
            params![url],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    fn quickmark_tag(slot: u8) -> String {
        format!("__quickmark:{}__", slot)
    }

    pub fn set_quickmark(&self, slot: u8, url: &str, title: &str) -> Result<(), rusqlite::Error> {
        let tag = Self::quickmark_tag(slot);
        // Remove any existing bookmark with this quickmark tag
        self.conn.execute(
            "DELETE FROM bookmarks WHERE tags LIKE ?1",
            params![format!("%{}%", tag)],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO bookmarks (url, title, tags) VALUES (?1, ?2, ?3)",
            params![url, title, tag],
        )?;
        Ok(())
    }

    pub fn get_quickmark(&self, slot: u8) -> Result<Option<Bookmark>, rusqlite::Error> {
        let tag = Self::quickmark_tag(slot);
        let mut stmt = self.conn.prepare(
            "SELECT url, title, tags, created_at FROM bookmarks WHERE tags LIKE ?1 LIMIT 1"
        )?;
        let mut entries = stmt.query_map(params![format!("%{}%", tag)], |row| {
            Ok(Bookmark {
                url: row.get(0)?,
                title: row.get(1)?,
                tags: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(entries.pop())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn make_bookmarks() -> BookmarkManager {
        let conn = db::open_database_in_memory().unwrap();
        BookmarkManager::new(conn)
    }

    #[test]
    fn add_and_search() {
        let bm = make_bookmarks();
        bm.add("https://example.com", "Example", "test,demo").unwrap();
        let results = bm.search("example", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://example.com");
        assert_eq!(results[0].tags, "test,demo");
    }

    #[test]
    fn search_by_tag() {
        let bm = make_bookmarks();
        bm.add("https://example.com", "Example", "rust,dev").unwrap();
        let results = bm.search("rust", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn remove_bookmark() {
        let bm = make_bookmarks();
        bm.add("https://example.com", "Example", "").unwrap();
        assert!(bm.is_bookmarked("https://example.com").unwrap());
        bm.remove("https://example.com").unwrap();
        assert!(!bm.is_bookmarked("https://example.com").unwrap());
    }

    #[test]
    fn add_duplicate_url_updates() {
        let bm = make_bookmarks();
        bm.add("https://example.com", "Old Title", "").unwrap();
        bm.add("https://example.com", "New Title", "updated").unwrap();
        let results = bm.search("example", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "New Title");
    }

    #[test]
    fn list_all_ordered_by_recency() {
        let bm = make_bookmarks();
        bm.add("https://a.com", "A", "").unwrap();
        bm.add("https://b.com", "B", "").unwrap();
        let all = bm.list_all(10).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].url, "https://b.com");
    }

    #[test]
    fn quickmark_set_and_get() {
        let bm = make_bookmarks();
        bm.set_quickmark(1, "https://example.com", "Example").unwrap();
        let qm = bm.get_quickmark(1).unwrap();
        assert!(qm.is_some());
        assert_eq!(qm.unwrap().url, "https://example.com");
    }

    #[test]
    fn quickmark_overwrites_same_slot() {
        let bm = make_bookmarks();
        bm.set_quickmark(2, "https://old.com", "Old").unwrap();
        bm.set_quickmark(2, "https://new.com", "New").unwrap();
        let qm = bm.get_quickmark(2).unwrap();
        assert_eq!(qm.unwrap().url, "https://new.com");
    }
}
