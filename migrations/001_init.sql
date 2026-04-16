CREATE TABLE IF NOT EXISTS sessions (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL DEFAULT 'default',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tiles (
    id          INTEGER PRIMARY KEY,
    session_id  INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    view_id     INTEGER NOT NULL,
    url         TEXT NOT NULL,
    title       TEXT NOT NULL DEFAULT '',
    scroll_x    REAL NOT NULL DEFAULT 0.0,
    scroll_y    REAL NOT NULL DEFAULT 0.0
);

CREATE TABLE IF NOT EXISTS layout_tree (
    id              INTEGER PRIMARY KEY,
    session_id      INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    node_index      INTEGER NOT NULL,
    is_leaf         INTEGER NOT NULL,
    direction       TEXT,
    ratio           REAL,
    view_id         INTEGER,
    focused_view_id INTEGER,
    UNIQUE(session_id, node_index)
);
