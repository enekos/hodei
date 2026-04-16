CREATE TABLE IF NOT EXISTS history (
    id              INTEGER PRIMARY KEY,
    url             TEXT NOT NULL,
    title           TEXT NOT NULL DEFAULT '',
    visit_count     INTEGER NOT NULL DEFAULT 1,
    last_visited_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_history_url ON history(url);
CREATE INDEX IF NOT EXISTS idx_history_last_visited ON history(last_visited_at);

CREATE TABLE IF NOT EXISTS bookmarks (
    id         INTEGER PRIMARY KEY,
    url        TEXT NOT NULL UNIQUE,
    title      TEXT NOT NULL DEFAULT '',
    tags       TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_bookmarks_url ON bookmarks(url);
