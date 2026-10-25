-- Phase 10.1 (REQ-VLT-010, REQ-VLT-011): direct email sharing + access log.
--
--   vault_shares  user-to-user share by email (reader|writer), with a
--                 per-share `hidden` flag ("Remove from my view") and a
--                 vault-level `allow_reshare` gate on the vaults table.
--   access_log    lightweight best-effort hit log feeding the per-vault
--                 statistics page; pruned to 90 days at boot.

ALTER TABLE vaults ADD COLUMN allow_reshare INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vaults ADD COLUMN last_refreshed_at TEXT;

CREATE TABLE IF NOT EXISTS vault_shares (
    vault_id INTEGER NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('reader', 'writer')),
    hidden INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (vault_id, email)
);

CREATE INDEX IF NOT EXISTS idx_vault_shares_email ON vault_shares(email);

CREATE TABLE IF NOT EXISTS access_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    vault_slug TEXT NOT NULL,
    email TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('web', 'mcp')),
    path TEXT NOT NULL,
    ts TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_access_log_slug_ts ON access_log(vault_slug, ts);
CREATE INDEX IF NOT EXISTS idx_access_log_email ON access_log(email, vault_slug);
