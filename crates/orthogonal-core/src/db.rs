use std::path::Path;
use std::rc::Rc;
use rusqlite::Connection;

const MIGRATION_001: &str = include_str!("../../../migrations/001_init.sql");
const MIGRATION_002: &str = include_str!("../../../migrations/002_history_bookmarks.sql");

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
}
