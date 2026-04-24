use std::path::Path;
use std::rc::Rc;
use rusqlite::Connection;

const MIGRATION_001: &str = include_str!("../../../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../../../migrations/002_history_bookmarks.sql");
const MIGRATION_003: &str = include_str!("../../../migrations/003_workspaces.sql");

pub fn open_database(path: &Path) -> Result<Rc<Connection>, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    run_migrations(&conn)?;
    Ok(Rc::new(conn))
}

pub fn open_database_in_memory() -> Result<Rc<Connection>, rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    run_migrations(&conn)?;
    Ok(Rc::new(conn))
}

fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(MIGRATION_001)?;
    conn.execute_batch(MIGRATION_002)?;
    // Migration 003 adds columns - check if they already exist
    let has_is_active: bool = conn
        .prepare("SELECT is_active FROM sessions LIMIT 0")
        .is_ok();
    if !has_is_active {
        conn.execute_batch(MIGRATION_003)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_creates_all_tables() {
        let conn = open_database_in_memory().unwrap();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"tiles".to_string()));
        assert!(tables.contains(&"layout_tree".to_string()));
        assert!(tables.contains(&"history".to_string()));
        assert!(tables.contains(&"bookmarks".to_string()));
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = open_database_in_memory().unwrap();
        // Running migrations again should not fail
        run_migrations(&conn).unwrap();
    }

    #[test]
    fn open_on_disk_then_reopen_preserves_data() {
        let dir = std::env::temp_dir().join("hodei-test-db-reopen");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        std::fs::remove_file(&path).ok();

        {
            let conn = open_database(&path).unwrap();
            conn.execute(
                "INSERT INTO history (url, title) VALUES (?1, ?2)",
                rusqlite::params!["https://preserved.test", "preserved"],
            )
            .unwrap();
        }

        let conn = open_database(&path).unwrap();
        let title: String = conn
            .query_row(
                "SELECT title FROM history WHERE url = ?1",
                rusqlite::params!["https://preserved.test"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, "preserved");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let conn = open_database_in_memory().unwrap();
        // tiles has FK session_id -> sessions(id). Inserting with a bogus
        // session_id should fail when foreign_keys=ON.
        let err = conn.execute(
            "INSERT INTO tiles (session_id, view_id, url, title, scroll_x, scroll_y) \
             VALUES (999, 1, '', '', 0, 0)",
            [],
        );
        assert!(err.is_err(), "expected FK violation, got {err:?}");
    }

    #[test]
    fn sessions_table_has_is_active_after_migration_003() {
        let conn = open_database_in_memory().unwrap();
        // Migration 003 adds is_active — smoke test it's present.
        assert!(conn.prepare("SELECT is_active FROM sessions LIMIT 0").is_ok());
    }
}
