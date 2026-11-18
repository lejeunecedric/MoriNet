-- Phase 19 (STAT-01, D-07): widen access_log.source CHECK to include 'api' —
-- first table rebuild in the project; history preserved.
--
--   access_log      rebuilt create-new/copy/drop/rename (SQLite cannot ALTER a
--                   CHECK): column-for-column identical to 0008 except the
--                   CHECK vocabulary is now ('web', 'mcp', 'api'). The explicit
--                   column-list INSERT...SELECT copies every pre-existing row
--                   including its id; both original indexes are recreated after
--                   the rename. sqlx wraps this file in a transaction, so the
--                   rebuild is atomic — no explicit transaction statements here.
--   mcp_activity    Phase 19 (D-01/D-02): one row per vault-scoped MCP tool
--                   call — request (tool + truncated args) and outcome
--                   (status, latency, size, ~200-char excerpt). Full response
--                   bodies are NEVER stored (STAT-05): there is no column that
--                   could hold one.
--   search_queries  Phase 19 (D-04): raw query text per search, one row per
--                   searched vault, source mcp|api.

CREATE TABLE access_log_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    vault_slug TEXT NOT NULL,
    email TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('web', 'mcp', 'api')),
    path TEXT NOT NULL,
    ts TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO access_log_new (id, vault_slug, email, source, path, ts)
    SELECT id, vault_slug, email, source, path, ts FROM access_log;

DROP TABLE access_log;

ALTER TABLE access_log_new RENAME TO access_log;

CREATE INDEX idx_access_log_slug_ts ON access_log(vault_slug, ts);
CREATE INDEX idx_access_log_email ON access_log(email, vault_slug);

CREATE TABLE IF NOT EXISTS mcp_activity (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    vault_slug TEXT NOT NULL,
    email TEXT NOT NULL,
    tool TEXT NOT NULL CHECK (tool IN ('list_notes', 'read_note', 'search_notes')),
    args TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('ok', 'error')),
    latency_ms INTEGER NOT NULL DEFAULT 0,
    resp_size INTEGER NOT NULL DEFAULT 0,
    excerpt TEXT NOT NULL DEFAULT '',
    ts TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_mcp_activity_slug_ts ON mcp_activity(vault_slug, ts);

CREATE TABLE IF NOT EXISTS search_queries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    vault_slug TEXT NOT NULL,
    email TEXT NOT NULL,
    query TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('mcp', 'api')),
    ts TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_search_queries_slug_ts ON search_queries(vault_slug, ts);
