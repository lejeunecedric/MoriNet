-- Phase 13 (REQ-CHG-001, REQ-CHG-002): wiki changes log.
--
--   vault_changes  best-effort log of file-level changes a refresh (or a
--                  WebDAV PUT) applied to a vault: one row per changed
--                  repo-relative path, all rows of one refresh sharing one
--                  event_id; pruned to 90 days at boot like access_log.
--                  `kind` is CHECK-locked to the D-08 vocabulary.

CREATE TABLE IF NOT EXISTS vault_changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    vault_slug TEXT NOT NULL,
    event_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('added', 'modified', 'removed')),
    path TEXT NOT NULL,
    ts TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_vault_changes_slug_ts ON vault_changes(vault_slug, ts);
