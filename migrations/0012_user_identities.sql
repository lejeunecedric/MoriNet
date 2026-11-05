-- Phase 17 (SSO-05/SSO-06, D-11, D-05): generic OIDC SSO foundations.
--
--   user_identities  links an external OIDC identity (provider, iss, sub) to
--                  a local user; (iss, sub) is the authoritative identity
--                  after first link (D-03), and UNIQUE(provider, iss, sub)
--                  guarantees one IdP account maps to at most one local
--                  account (T-17-02). `users` is untouched (D-11: additive
--                  CREATE TABLE only, no ALTER, no table rebuild).
--   auth_events    best-effort audit log for auth events (link, provision,
--                  deny, demote, group_sync — CHECK-locked vocab, D-05).
--                  Carries actor + iss + sub + short detail ONLY: no token,
--                  code, or secret column exists at all (T-17-04); pruned to
--                  90 days at boot like access_log / vault_changes.

CREATE TABLE IF NOT EXISTS user_identities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    iss TEXT NOT NULL,
    sub TEXT NOT NULL,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    linked_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(provider, iss, sub)
);

CREATE TABLE IF NOT EXISTS auth_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts TEXT NOT NULL DEFAULT (datetime('now')),
    kind TEXT NOT NULL CHECK (kind IN ('link', 'provision', 'deny', 'demote', 'group_sync')),
    actor_email TEXT NOT NULL DEFAULT '',
    iss TEXT NOT NULL DEFAULT '',
    sub TEXT NOT NULL DEFAULT '',
    detail TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_auth_events_ts ON auth_events(ts);
