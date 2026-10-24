-- API tokens for MCP Bearer auth (Phase 10, PLAN 10-01).
-- Stored HASHED (sha256 hex) like invitations/reset_tokens — the plaintext is
-- shown once at creation and never persisted.

CREATE TABLE IF NOT EXISTS api_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,             -- unix seconds
    last_used_at INTEGER                     -- unix seconds, NULL until first use
);
