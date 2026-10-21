-- Groups + personal vaults (Phase 8 — access-model).
--
-- Replaces the flat `vault_acl` model with:
--   groups           named, admin-managed
--   group_members    user <-> group
--   vault_group_acl  role (read|write) per (vault, group)
--
-- Existing vaults become scope='global' with no owner, so nothing that works
-- today stops working. `vault_acl` is kept for one release as a fallback and
-- dropped later — the readers of this file should not treat it as the source
-- of truth anymore.

ALTER TABLE vaults ADD COLUMN scope TEXT NOT NULL DEFAULT 'global'
    CHECK (scope IN ('global', 'personal'));
ALTER TABLE vaults ADD COLUMN owner_id INTEGER REFERENCES users(id) ON DELETE CASCADE;

-- Display name shown in the UI; the email stays the login identifier.
ALTER TABLE users ADD COLUMN display_name TEXT;

CREATE TABLE groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE group_members (
    group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, user_id)
);

CREATE TABLE vault_group_acl (
    vault_id INTEGER NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'read' CHECK (role IN ('read', 'write')),
    PRIMARY KEY (vault_id, group_id)
);

CREATE INDEX idx_vaults_owner ON vaults(owner_id);
CREATE INDEX idx_vaults_scope ON vaults(scope);
CREATE INDEX idx_group_members_user ON group_members(user_id);
CREATE INDEX idx_vault_group_acl_group ON vault_group_acl(group_id);