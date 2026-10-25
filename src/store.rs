//! SQLite persistence layer (Phase 2): users, vaults, per-user ACL.
//!
//! Schema is versioned via `sqlx::migrate!("./migrations")` (REQ-OPS-004).

use std::path::Path;

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

/// Minimal bcrypt cost for test fixtures only (default cost 12 is ~200ms).
#[cfg(test)]
pub const TEST_BCRYPT_COST: u32 = 4;

#[derive(Debug, Clone)]
pub struct Vault {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub book_path: String,
    /// `global` (shared through groups) or `personal` (solo, owner only).
    pub scope: String,
    /// Owner email for a personal vault; None for a global one.
    pub owner_email: Option<String>,
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub role: String,
    /// Shown in the UI. `None` until the user sets one; the email stays the
    /// login identifier.
    pub display_name: Option<String>,
}

/// An API token row for the `/account` listing — the hash is never exposed.
#[derive(Debug, Clone)]
pub struct ApiToken {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// A vault as seen by a user, with the role they hold on it.
#[derive(Debug, Clone)]
pub struct AccessibleVault {
    pub vault: Vault,
    /// 'read' or 'write'. A personal vault always yields 'write' for its owner.
    pub role: String,
    pub personal: bool,
    /// True only on share-derived rows: the vault reaches this user through a
    /// direct email share, so the web layer can render "Remove from my view".
    pub direct_share: bool,
}

/// What the user may do on a vault. Ordering matters: `None < Read < Write`,
/// so `max()` picks the strongest grant when several sources overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    None,
    Read,
    Write,
}

impl Access {
    pub fn can_read(self) -> bool {
        matches!(self, Access::Read | Access::Write)
    }
    pub fn can_write(self) -> bool {
        matches!(self, Access::Write)
    }
    fn parse(role: &str) -> Access {
        match role {
            "write" => Access::Write,
            "read" => Access::Read,
            _ => Access::None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    pub pool: SqlitePool,
}

impl Store {
    /// Open (or create) the SQLite database at `db_path` and run migrations.
    pub async fn connect(db_path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    /// In-memory store for tests.
    pub async fn connect_memory() -> anyhow::Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    /// Idempotently create the bootstrap admin (REQ-AUTH-001, staging-grade env creds).
    pub async fn ensure_admin(&self, email: &str, password: &str) -> anyhow::Result<()> {
        let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        if existing.is_some() {
            return Ok(());
        }
        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;
        sqlx::query("INSERT INTO users (email, password_hash, role) VALUES (?, ?, 'admin')")
            .bind(email)
            .bind(hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Idempotently register a vault; updates the book path if it changed.
    pub async fn seed_vault(&self, slug: &str, name: &str, book_path: &Path) -> anyhow::Result<()> {
        let path_str = book_path.display().to_string();
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT book_path FROM vaults WHERE slug = ?")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await?;
        match existing {
            None => {
                sqlx::query(
                    "INSERT INTO vaults (slug, name, book_path, last_refreshed_at) \
                     VALUES (?, ?, ?, datetime('now'))",
                )
                .bind(slug)
                .bind(name)
                .bind(&path_str)
                .execute(&self.pool)
                .await?;
            }
            Some((old_path,)) if old_path != path_str => {
                sqlx::query(
                    "UPDATE vaults SET name = ?, book_path = ?, last_refreshed_at = datetime('now') \
                     WHERE slug = ?",
                )
                .bind(name)
                .bind(&path_str)
                .bind(slug)
                .execute(&self.pool)
                .await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Verify credentials against the users table (bcrypt). Returns the user on success.
    pub async fn verify_login(&self, email: &str, password: &str) -> anyhow::Result<Option<User>> {
        let row: Option<(i64, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, email, password_hash, role, display_name FROM users WHERE email = ?",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some((id, email, hash, role, display_name)) => {
                if bcrypt::verify(password, &hash)? {
                    Ok(Some(User {
                        id,
                        email,
                        role,
                        display_name,
                    }))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// List vaults visible to `email`: admins see all, readers see ACL-granted ones.
    pub async fn list_vaults_for(&self, email: &str) -> anyhow::Result<Vec<Vault>> {
        let role: Option<(String,)> = sqlx::query_as("SELECT role FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        match role {
            None => Ok(vec![]),
            Some((r,)) if r == "admin" => self.list_all_vaults().await,
            Some(_) => {
                let rows: Vec<(i64, String, String, String, String, Option<String>)> =
                    sqlx::query_as(
                        "SELECT v.id, v.slug, v.name, v.book_path, v.scope, owner.email \
                     FROM vaults v \
                     JOIN vault_acl a ON a.vault_id = v.id \
                     JOIN users u ON u.id = a.user_id \
                     LEFT JOIN users owner ON owner.id = v.owner_id \
                     WHERE u.email = ? \
                     ORDER BY v.slug",
                    )
                    .bind(email)
                    .fetch_all(&self.pool)
                    .await?;
                Ok(rows
                    .into_iter()
                    .map(|(id, slug, name, book_path, scope, owner_email)| Vault {
                        id,
                        slug,
                        name,
                        book_path,
                        scope,
                        owner_email,
                    })
                    .collect())
            }
        }
    }

    pub async fn list_all_vaults(&self) -> anyhow::Result<Vec<Vault>> {
        let rows: Vec<(i64, String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT v.id, v.slug, v.name, v.book_path, v.scope, u.email
             FROM vaults v LEFT JOIN users u ON u.id = v.owner_id
             ORDER BY v.scope, v.slug",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, slug, name, book_path, scope, owner_email)| Vault {
                id,
                slug,
                name,
                book_path,
                scope,
                owner_email,
            })
            .collect())
    }

    /// Whether `email` may access the vault with the given slug (REQ-VAULT-004).
    /// Admins may access any vault that exists; readers need an ACL row.
    pub async fn acl_allows(&self, email: &str, slug: &str) -> anyhow::Result<bool> {
        let row: Option<(String,)> = sqlx::query_as("SELECT u.role FROM users u WHERE u.email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        let role = match row {
            Some((r,)) => r,
            None => return Ok(false),
        };
        if role == "admin" {
            let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM vaults WHERE slug = ?")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await?;
            return Ok(exists.is_some());
        }
        let granted: Option<(i64,)> = sqlx::query_as(
            "SELECT v.id \
             FROM vaults v \
             JOIN vault_acl a ON a.vault_id = v.id \
             JOIN users u ON u.id = a.user_id \
             WHERE u.email = ? AND v.slug = ?",
        )
        .bind(email)
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        Ok(granted.is_some())
    }

    /// Grant a user access to a vault (idempotent).
    pub async fn grant_acl(&self, email: &str, slug: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO vault_acl (vault_id, user_id) \
             SELECT v.id, u.id FROM vaults v, users u WHERE v.slug = ? AND u.email = ?",
        )
        .bind(slug)
        .bind(email)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Revoke a user's access to a vault (idempotent).
    pub async fn revoke_acl(&self, email: &str, slug: &str) -> anyhow::Result<()> {
        sqlx::query(
            "DELETE FROM vault_acl \
             WHERE user_id = (SELECT id FROM users WHERE email = ?) \
             AND vault_id = (SELECT id FROM vaults WHERE slug = ?)",
        )
        .bind(email)
        .bind(slug)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Create a reader user directly (tests / future admin-ui).
    pub async fn create_user(&self, email: &str, password: &str, role: &str) -> anyhow::Result<()> {
        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;
        sqlx::query("INSERT INTO users (email, password_hash, role) VALUES (?, ?, ?)")
            .bind(email)
            .bind(hash)
            .bind(role)
            .execute(&self.pool)
            .await?;
        // Join the default plan group when the builtin groups are provisioned
        // (REQ-GRP-006/007): readers get the `free` quota, admins the `admin`
        // one. A no-op when those groups don't exist (bare test stores).
        let default_group = if role == "admin" { "admin" } else { "free" };
        sqlx::query(
            "INSERT INTO group_members (group_id, user_id)
             SELECT g.id, u.id FROM groups g, users u
             WHERE g.name = ? AND u.email = ?
             ON CONFLICT DO NOTHING",
        )
        .bind(default_group)
        .bind(email)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Idempotently provision the builtin plan groups (REQ-GRP-006):
    /// `free` (2 personal vaults) and `admin` (unlimited).
    pub async fn ensure_builtin_groups(&self) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO groups (name, personal_vault_limit) VALUES ('free', 2)
             ON CONFLICT(name) DO NOTHING",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO groups (name, personal_vault_limit) VALUES ('admin', NULL)
             ON CONFLICT(name) DO NOTHING",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Idempotent boot sweep (REQ-GRP-007): every account lands in its default
    /// plan group — admins in `admin`, groupless readers in `free`. A user an
    /// admin already placed in a group is left untouched.
    pub async fn ensure_default_memberships(&self) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO group_members (group_id, user_id)
             SELECT g.id, u.id FROM groups g, users u
             WHERE g.name = 'admin' AND u.role = 'admin'
             ON CONFLICT DO NOTHING",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO group_members (group_id, user_id)
             SELECT g.id, u.id FROM groups g, users u
             WHERE g.name = 'free' AND u.role = 'reader'
               AND NOT EXISTS (SELECT 1 FROM group_members m WHERE m.user_id = u.id)
             ON CONFLICT DO NOTHING",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Anti-lockout fallback: if NO account has the admin role at boot, create
    /// (or re-promote) the well-known `admin`/`admin` account. Demoting the
    /// only admin would otherwise leave the instance unmanageable forever.
    /// A healthy instance (an admin exists) is left untouched.
    pub async fn ensure_fallback_admin(&self) -> anyhow::Result<()> {
        let any_admin: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM users WHERE role = 'admin' LIMIT 1")
                .fetch_optional(&self.pool)
                .await?;
        if any_admin.is_some() {
            return Ok(());
        }
        let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE email = 'admin'")
            .fetch_optional(&self.pool)
            .await?;
        if existing.is_some() {
            self.set_user_role("admin", "admin").await
        } else {
            self.create_user("admin", "admin", "admin").await
        }
    }

    /// Change a user's role (REQ-GRP-007) and fix their plan group: an admin
    /// joins `admin`; a reader leaves it and lands in `free` only when they
    /// have no other group. Non-default groups are left untouched.
    pub async fn set_user_role(&self, email: &str, role: &str) -> anyhow::Result<()> {
        anyhow::ensure!(role == "admin" || role == "reader", "unknown role {role}");
        sqlx::query("UPDATE users SET role = ? WHERE email = ?")
            .bind(role)
            .bind(email)
            .execute(&self.pool)
            .await?;
        if role == "reader" {
            sqlx::query(
                "DELETE FROM group_members
                 WHERE user_id = (SELECT id FROM users WHERE email = ?)
                   AND group_id = (SELECT id FROM groups WHERE name = 'admin')",
            )
            .bind(email)
            .execute(&self.pool)
            .await?;
        }
        let default_group = if role == "admin" { "admin" } else { "free" };
        sqlx::query(
            "INSERT INTO group_members (group_id, user_id)
             SELECT g.id, u.id FROM groups g, users u
             WHERE g.name = ? AND u.email = ?
               AND (u.role = 'admin'
                    OR NOT EXISTS (SELECT 1 FROM group_members m WHERE m.user_id = u.id))
             ON CONFLICT DO NOTHING",
        )
        .bind(default_group)
        .bind(email)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// List all users (admin-ui table).
    pub async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        let rows: Vec<(i64, String, String, Option<String>)> =
            sqlx::query_as("SELECT id, email, role, display_name FROM users ORDER BY email")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|(id, email, role, display_name)| User {
                id,
                email,
                role,
                display_name,
            })
            .collect())
    }

    /// Set the display name shown in the UI (Phase 9, REQ-ACC-002).
    /// The display name for the account menu (falls back to None → email).
    pub async fn display_name_for(&self, email: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT display_name FROM users WHERE email = ?")
                .bind(email)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(n,)| n))
    }

    pub async fn set_display_name(&self, email: &str, name: &str) -> anyhow::Result<()> {
        let trimmed = name.trim();
        let value = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        sqlx::query("UPDATE users SET display_name = ? WHERE email = ?")
            .bind(value)
            .bind(email)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Role of a user (None if unknown email) — used by the admin gate.
    pub async fn role_for(&self, email: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as("SELECT role FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(r,)| r))
    }

    /// Delete a vault and its ACL rows (FK ON DELETE CASCADE handles the ACL).
    pub async fn delete_vault(&self, slug: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM vaults WHERE slug = ?")
            .bind(slug)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---------- invitation tokens (Phase 5, REQ-EMAIL-005) ----------

    /// Store an invitation token (hashed). Single-use, expires after `ttl_secs`.
    pub async fn create_invitation(
        &self,
        token_hash: &str,
        email: &str,
        role: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        let expires_at = (self.unix_now() + ttl_secs) as i64;
        sqlx::query(
            "INSERT INTO invitations (token_hash, email, role, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(token_hash)
        .bind(email)
        .bind(role)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Validate an invitation token: returns (email, role) if valid and unused.
    pub async fn validate_invitation(
        &self,
        token_hash: &str,
    ) -> anyhow::Result<Option<(String, String)>> {
        let row: Option<(String, String, i64, Option<i64>)> = sqlx::query_as(
            "SELECT email, role, expires_at, used_at FROM invitations WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some((email, role, expires_at, used_at))
                if used_at.is_none() && (expires_at as u64) > self.unix_now() =>
            {
                Ok(Some((email, role)))
            }
            _ => Ok(None),
        }
    }

    /// Consume an invitation: create the user with the given password, mark used.
    pub async fn consume_invitation(
        &self,
        token_hash: &str,
        password: &str,
    ) -> anyhow::Result<Option<String>> {
        let Some((email, role)) = self.validate_invitation(token_hash).await? else {
            return Ok(None);
        };
        self.create_user(&email, password, &role).await?;
        sqlx::query("UPDATE invitations SET used_at = ? WHERE token_hash = ?")
            .bind(self.unix_now() as i64)
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(Some(email))
    }

    // ---------- password reset tokens (REQ-AUTH-005) ----------

    /// Store a reset token (hashed) for a user. No user → no token (don't leak existence).
    pub async fn create_reset_token(
        &self,
        token_hash: &str,
        email: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        let user: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        let Some((user_id,)) = user else {
            return Ok(());
        };
        let expires_at = (self.unix_now() + ttl_secs) as i64;
        sqlx::query("INSERT INTO reset_tokens (token_hash, user_id, expires_at) VALUES (?, ?, ?)")
            .bind(token_hash)
            .bind(user_id)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Validate a reset token → true if valid and unused and not expired.
    pub async fn validate_reset_token(&self, token_hash: &str) -> anyhow::Result<Option<i64>> {
        let row: Option<(i64, i64, Option<i64>)> = sqlx::query_as(
            "SELECT user_id, expires_at, used_at FROM reset_tokens WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some((user_id, expires_at, used_at))
                if used_at.is_none() && (expires_at as u64) > self.unix_now() =>
            {
                Ok(Some(user_id))
            }
            _ => Ok(None),
        }
    }

    /// Consume a reset token: set the new password, mark used.
    pub async fn consume_reset_token(
        &self,
        token_hash: &str,
        password: &str,
    ) -> anyhow::Result<Option<i64>> {
        let Some(user_id) = self.validate_reset_token(token_hash).await? else {
            return Ok(None);
        };
        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE reset_tokens SET used_at = ? WHERE token_hash = ?")
            .bind(self.unix_now() as i64)
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(Some(user_id))
    }

    // ---------- Phase 8: groups + personal vaults ----------

    /// Effective access of `email` on `slug`.
    ///
    /// Order matters:
    ///   1. admin sees everything (existing behaviour);
    ///   2. the owner of a personal vault always has Write on it;
    ///   3. otherwise the strongest role across the user's groups wins.
    pub async fn access_for(&self, email: &str, slug: &str) -> anyhow::Result<Access> {
        let row: Option<(String, Option<i64>)> =
            sqlx::query_as("SELECT v.scope, v.owner_id FROM vaults v WHERE v.slug = ?")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await?;
        let Some((_scope, owner_id)) = row else {
            return Ok(Access::None);
        };

        let user: Option<(i64, String)> =
            sqlx::query_as("SELECT id, role FROM users WHERE email = ?")
                .bind(email)
                .fetch_optional(&self.pool)
                .await?;
        let Some((user_id, role)) = user else {
            return Ok(Access::None);
        };
        if role == "admin" {
            return Ok(Access::Write);
        }
        // Owner of a personal vault: full rights, no group row needed.
        if owner_id == Some(user_id) {
            return Ok(Access::Write);
        }

        let best: Option<(String,)> = sqlx::query_as(
            "SELECT a.role FROM vault_group_acl a
             JOIN group_members m ON m.group_id = a.group_id
             WHERE m.user_id = ? AND a.vault_id = (SELECT id FROM vaults WHERE slug = ?)
             ORDER BY CASE a.role WHEN 'write' THEN 0 ELSE 1 END
             LIMIT 1",
        )
        .bind(user_id)
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        let group_access = best.map(|(r,)| Access::parse(&r)).unwrap_or(Access::None);

        // 4. a direct email share (not hidden): reader → Read, writer → Write.
        //    The stronger of the group grant and the share wins.
        let share: Option<(String,)> = sqlx::query_as(
            "SELECT s.role FROM vault_shares s
             WHERE s.email = ? AND s.hidden = 0
               AND s.vault_id = (SELECT id FROM vaults WHERE slug = ?)",
        )
        .bind(email)
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        let share_access = share
            .map(|(r,)| Access::parse(if r == "writer" { "write" } else { "read" }))
            .unwrap_or(Access::None);
        Ok(group_access.max(share_access))
    }

    /// Vaults visible to `email`: their personal ones + the global ones their
    /// groups are allowed to read. Admins get everything (Phase 8).
    pub async fn accessible_vaults(&self, email: &str) -> anyhow::Result<Vec<AccessibleVault>> {
        let user: Option<(i64, String)> =
            sqlx::query_as("SELECT id, role FROM users WHERE email = ?")
                .bind(email)
                .fetch_optional(&self.pool)
                .await?;
        let Some((user_id, role)) = user else {
            return Ok(vec![]);
        };
        let is_admin = role == "admin";

        // Admins: everything.
        if is_admin {
            let rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(
                "SELECT v.id, v.slug, v.name, v.book_path, v.scope
                 FROM vaults v ORDER BY v.scope DESC, v.slug",
            )
            .fetch_all(&self.pool)
            .await?;
            return Ok(rows
                .into_iter()
                .map(|(id, slug, name, book_path, scope)| AccessibleVault {
                    vault: Vault {
                        id,
                        slug,
                        name,
                        book_path,
                        scope: scope.clone(),
                        owner_email: None,
                    },
                    role: "write".to_string(),
                    personal: scope == "personal",
                    direct_share: false,
                })
                .collect());
        }

        let mut out: Vec<AccessibleVault> = Vec::new();

        // Personal vaults owned by this user.
        let own: Vec<(i64, String, String, String, String)> = sqlx::query_as(
            "SELECT v.id, v.slug, v.name, v.book_path, u.email
             FROM vaults v JOIN users u ON u.id = v.owner_id
             WHERE v.scope = 'personal' AND v.owner_id = ?
             ORDER BY v.slug",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        for (id, slug, name, book_path, email) in own {
            out.push(AccessibleVault {
                vault: Vault {
                    id,
                    slug,
                    name,
                    book_path,
                    scope: "personal".to_string(),
                    owner_email: Some(email),
                },
                role: "write".to_string(),
                personal: true,
                direct_share: false,
            });
        }

        // Global vaults granted through any of the user's groups. The strongest
        // role per vault wins, hence the GROUP BY + MAX-like CASE.
        let globals: Vec<(i64, String, String, String, String)> = sqlx::query_as(
            "SELECT v.id, v.slug, v.name, v.book_path,
                    CASE WHEN MAX(CASE a.role WHEN 'write' THEN 1 ELSE 0 END) = 1
                         THEN 'write' ELSE 'read' END AS role
             FROM vaults v
             JOIN vault_group_acl a ON a.vault_id = v.id
             JOIN group_members m ON m.group_id = a.group_id
             WHERE v.scope = 'global' AND m.user_id = ?
             GROUP BY v.id
             ORDER BY v.slug",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        for (id, slug, name, book_path, role) in globals {
            out.push(AccessibleVault {
                vault: Vault {
                    id,
                    slug,
                    name,
                    book_path,
                    scope: "global".to_string(),
                    owner_email: None,
                },
                role,
                personal: false,
                direct_share: false,
            });
        }

        // Direct email shares (not hidden). A vault already present (personal
        // or group-granted) is NOT duplicated — the stronger role wins; a
        // share-only row carries direct_share=true for the web layer.
        type ShareRow = (i64, String, String, String, String, String, Option<String>);
        let shares: Vec<ShareRow> = sqlx::query_as(
            "SELECT v.id, v.slug, v.name, v.book_path, v.scope, s.role, ou.email \
                 FROM vault_shares s \
                 JOIN vaults v ON v.id = s.vault_id \
                 LEFT JOIN users ou ON ou.id = v.owner_id \
                 WHERE s.email = ? AND s.hidden = 0 \
                 ORDER BY v.slug",
        )
        .bind(email)
        .fetch_all(&self.pool)
        .await?;
        for (id, slug, name, book_path, scope, share_role, owner_email) in shares {
            let role = if share_role == "writer" {
                "write"
            } else {
                "read"
            };
            if let Some(existing) = out.iter_mut().find(|av| av.vault.slug == slug) {
                if role == "write" && existing.role != "write" {
                    existing.role = "write".to_string();
                }
                continue;
            }
            out.push(AccessibleVault {
                vault: Vault {
                    id,
                    slug,
                    name,
                    book_path,
                    scope,
                    owner_email,
                },
                role: role.to_string(),
                personal: false,
                direct_share: true,
            });
        }
        Ok(out)
    }

    /// Create a personal vault owned by `email` (REQ-VLT-005), respecting the
    /// caller's group quota (REQ-VLT-008).
    pub async fn create_personal_vault(
        &self,
        email: &str,
        slug: &str,
        name: &str,
        book_path: &Path,
    ) -> anyhow::Result<()> {
        let owner: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        let Some((owner_id,)) = owner else {
            anyhow::bail!("cannot create a vault for an unknown user");
        };
        // Quota (REQ-VLT-008): a member cannot create more personal vaults than
        // their most permissive group allows. Shared/global vaults never count.
        if let Some(limit) = self.effective_personal_vault_limit(email).await? {
            let used = self.count_personal_vaults(email).await?;
            if used >= limit {
                anyhow::bail!(
                    "personal vault limit reached: {used}/{limit} — ask an admin for a larger group"
                );
            }
        }
        let path_str = book_path.display().to_string();
        sqlx::query(
            "INSERT INTO vaults (slug, name, book_path, scope, owner_id)
             VALUES (?, ?, ?, 'personal', ?)",
        )
        .bind(slug)
        .bind(name)
        .bind(path_str)
        .bind(owner_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a PERSONAL vault owned by `email` — owner-only, and the freed
    /// quota slot is immediately reusable. Returns whether a row went away.
    pub async fn delete_personal_vault(&self, email: &str, slug: &str) -> anyhow::Result<bool> {
        let res = sqlx::query(
            "DELETE FROM vaults
             WHERE slug = ? AND scope = 'personal'
               AND owner_id = (SELECT id FROM users WHERE email = ?)",
        )
        .bind(slug)
        .bind(email)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// The upstream URL a vault was loaded from (None = local/legacy).
    pub async fn vault_source(&self, slug: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT source_url FROM vaults WHERE slug = ?")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(v,)| v))
    }

    /// Remember where a vault came from (set at load time).
    pub async fn set_vault_source(&self, slug: &str, url: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE vaults SET source_url = ? WHERE slug = ?")
            .bind(url)
            .bind(slug)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The vault name (for rebuilds).
    pub async fn vault_name(&self, slug: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as("SELECT name FROM vaults WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(n,)| n))
    }

    /// When the vault was last refreshed/re-synced (stats panel, REQ-VLT-011).
    pub async fn vault_last_refreshed(&self, slug: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT last_refreshed_at FROM vaults WHERE slug = ?")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(t,)| t))
    }

    /// How many personal vaults `email` owns (REQ-VLT-008).
    pub async fn count_personal_vaults(&self, email: &str) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM vaults v
             JOIN users u ON u.id = v.owner_id
             WHERE u.email = ? AND v.scope = 'personal'",
        )
        .bind(email)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }

    /// How many personal vaults `email` may create (REQ-GRP-008).
    /// `None` = unlimited (a user in no group, or in a group with a NULL limit).
    /// The most permissive group wins: NULL beats any number, otherwise max.
    pub async fn effective_personal_vault_limit(&self, email: &str) -> anyhow::Result<Option<i64>> {
        let limits: Vec<(Option<i64>,)> = sqlx::query_as(
            "SELECT g.personal_vault_limit
             FROM groups g
             JOIN group_members m ON m.group_id = g.id
             JOIN users u ON u.id = m.user_id
             WHERE u.email = ?",
        )
        .bind(email)
        .fetch_all(&self.pool)
        .await?;
        if limits.iter().any(|(l,)| l.is_none()) {
            return Ok(None);
        }
        Ok(limits.into_iter().filter_map(|(l,)| l).max())
    }

    // ---------- groups ----------

    pub async fn create_group(&self, name: &str) -> anyhow::Result<i64> {
        let res = sqlx::query("INSERT INTO groups (name) VALUES (?)")
            .bind(name.trim())
            .execute(&self.pool)
            .await?;
        Ok(res.last_insert_rowid())
    }

    pub async fn list_groups(&self) -> anyhow::Result<Vec<(i64, String, i64, Option<i64>)>> {
        // (id, name, member count, personal_vault_limit) — limit NULL = unlimited.
        let rows: Vec<(i64, String, i64, Option<i64>)> = sqlx::query_as(
            "SELECT g.id, g.name, COUNT(m.user_id), g.personal_vault_limit
             FROM groups g LEFT JOIN group_members m ON m.group_id = g.id
             GROUP BY g.id ORDER BY g.name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Set the per-group personal vault creation limit (REQ-GRP-005).
    /// `None` = unlimited.
    pub async fn set_group_limit(&self, group_id: i64, limit: Option<i64>) -> anyhow::Result<()> {
        sqlx::query("UPDATE groups SET personal_vault_limit = ? WHERE id = ?")
            .bind(limit)
            .bind(group_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn rename_group(&self, group_id: i64, name: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE groups SET name = ? WHERE id = ?")
            .bind(name.trim())
            .bind(group_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Deleting a group cascades to its members AND its vault ACLs.
    pub async fn delete_group(&self, group_id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM groups WHERE id = ?")
            .bind(group_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_group_member(&self, group_id: i64, email: &str) -> anyhow::Result<bool> {
        let user: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        let Some((user_id,)) = user else {
            return Ok(false);
        };
        sqlx::query("INSERT OR IGNORE INTO group_members (group_id, user_id) VALUES (?, ?)")
            .bind(group_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(true)
    }

    pub async fn remove_group_member(&self, group_id: i64, email: &str) -> anyhow::Result<bool> {
        let res = sqlx::query(
            "DELETE FROM group_members
             WHERE group_id = ?
               AND user_id = (SELECT id FROM users WHERE email = ?)",
        )
        .bind(group_id)
        .bind(email)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Members of a group (email list, sorted).
    pub async fn group_members(&self, group_id: i64) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT u.email FROM group_members m
             JOIN users u ON u.id = m.user_id
             WHERE m.group_id = ? ORDER BY u.email",
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(e,)| e).collect())
    }

    /// Assign a group a role on a vault ('read' | 'write', REQ-VLT-004).
    pub async fn set_vault_group_role(
        &self,
        slug: &str,
        group_id: i64,
        role: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO vault_group_acl (vault_id, group_id, role)
             VALUES ((SELECT id FROM vaults WHERE slug = ?), ?, ?)
             ON CONFLICT (vault_id, group_id) DO UPDATE SET role = excluded.role",
        )
        .bind(slug)
        .bind(group_id)
        .bind(role)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_vault_group_role(&self, slug: &str, group_id: i64) -> anyhow::Result<()> {
        sqlx::query(
            "DELETE FROM vault_group_acl
             WHERE group_id = ? AND vault_id = (SELECT id FROM vaults WHERE slug = ?)",
        )
        .bind(group_id)
        .bind(slug)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Group id → role, for the admin vault-assignment UI.
    pub async fn vault_group_roles(
        &self,
        slug: &str,
    ) -> anyhow::Result<Vec<(i64, String, String)>> {
        let rows: Vec<(i64, String, String)> = sqlx::query_as(
            "SELECT a.group_id, g.name, a.role
             FROM vault_group_acl a JOIN groups g ON g.id = a.group_id
             WHERE a.vault_id = (SELECT id FROM vaults WHERE slug = ?)
             ORDER BY g.name",
        )
        .bind(slug)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    // ---------- Phase 10.1: direct email sharing (REQ-VLT-010) ----------

    /// Share a vault with an email as `reader` or `writer` (upsert).
    /// Re-sharing the same email updates the role and clears `hidden`.
    /// The email may be unknown — the share resolves when that user registers.
    pub async fn share_vault(&self, slug: &str, email: &str, role: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            role == "reader" || role == "writer",
            "unknown share role {role}"
        );
        let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM vaults WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;
        anyhow::ensure!(exists.is_some(), "unknown vault {slug}");
        sqlx::query(
            "INSERT INTO vault_shares (vault_id, email, role) \
             VALUES ((SELECT id FROM vaults WHERE slug = ?), ?, ?) \
             ON CONFLICT(vault_id, email) DO UPDATE SET role = excluded.role, hidden = 0",
        )
        .bind(slug)
        .bind(email)
        .bind(role)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The owner's share list for a vault: (email, role, hidden, created_at),
    /// hidden rows included (they only hide the vault for the sharee).
    pub async fn list_shares(
        &self,
        slug: &str,
    ) -> anyhow::Result<Vec<(String, String, bool, String)>> {
        let rows: Vec<(String, String, bool, String)> = sqlx::query_as(
            "SELECT s.email, s.role, s.hidden, s.created_at FROM vault_shares s \
             WHERE s.vault_id = (SELECT id FROM vaults WHERE slug = ?) \
             ORDER BY s.email",
        )
        .bind(slug)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Revoke a share outright (row deleted). `false` if there was no such share.
    pub async fn revoke_share(&self, slug: &str, email: &str) -> anyhow::Result<bool> {
        let res = sqlx::query(
            "DELETE FROM vault_shares \
             WHERE email = ? \
               AND vault_id = (SELECT id FROM vaults WHERE slug = ?)",
        )
        .bind(email)
        .bind(slug)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// "Remove from my view": hide the share for that sharee only. The row
    /// survives (the owner's share list is unchanged). `false` if no row.
    pub async fn hide_share(&self, slug: &str, email: &str) -> anyhow::Result<bool> {
        let res = sqlx::query(
            "UPDATE vault_shares SET hidden = 1 \
             WHERE email = ? \
               AND vault_id = (SELECT id FROM vaults WHERE slug = ?)",
        )
        .bind(email)
        .bind(slug)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// May `email` share the vault further? Admins and the owner always can;
    /// a sharee only when the vault's `allow_reshare` toggle is on AND their
    /// own share is present and not hidden.
    pub async fn can_share(&self, email: &str, slug: &str) -> anyhow::Result<bool> {
        if self.role_for(email).await?.as_deref() == Some("admin") {
            return Ok(true);
        }
        let owner: Option<(i64,)> = sqlx::query_as(
            "SELECT v.id FROM vaults v \
             JOIN users u ON u.id = v.owner_id \
             WHERE v.slug = ? AND u.email = ?",
        )
        .bind(slug)
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        if owner.is_some() {
            return Ok(true);
        }
        let sharee: Option<(i64,)> = sqlx::query_as(
            "SELECT v.id FROM vaults v \
             JOIN vault_shares s ON s.vault_id = v.id \
             WHERE v.slug = ? AND s.email = ? AND s.hidden = 0 AND v.allow_reshare = 1",
        )
        .bind(slug)
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(sharee.is_some())
    }

    /// Current value of the re-share toggle (prefills the owner's Edit form).
    pub async fn allow_reshare(&self, slug: &str) -> anyhow::Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT allow_reshare FROM vaults WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(v,)| v != 0).unwrap_or(false))
    }

    /// Vault-level re-share toggle (part of the owner's Edit form).
    pub async fn set_allow_reshare(&self, slug: &str, on: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE vaults SET allow_reshare = ? WHERE slug = ?")
            .bind(on)
            .bind(slug)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Rename a vault. The slug is deliberately NOT changed so existing links,
    /// ACL rows and shares stay valid.
    pub async fn rename_vault(&self, slug: &str, name: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE vaults SET name = ? WHERE slug = ?")
            .bind(name)
            .bind(slug)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Whether an account with this email exists (drives the "pending share"
    /// hint on the owner's share form).
    pub async fn user_exists(&self, email: &str) -> anyhow::Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    /// Boot sweep (like `ensure_default_memberships`): grant every existing
    /// group `read` on a vault. Idempotent — existing roles are never
    /// downgraded (INSERT OR IGNORE on the (vault_id, group_id) PK).
    pub async fn grant_vault_to_all_groups(&self, slug: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO vault_group_acl (vault_id, group_id, role) \
             SELECT v.id, g.id, 'read' FROM vaults v, groups g WHERE v.slug = ?",
        )
        .bind(slug)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ─── Access log (Phase 10.1, REQ-VLT-011) ────────────────────────────

    /// Record one page view. Callers spawn this fire-and-forget and ignore
    /// errors — stats must never break a request. `source` is 'web' or 'mcp'
    /// (CHECK-enforced by migration 0008).
    pub async fn log_access(
        &self,
        email: &str,
        slug: &str,
        source: &str,
        path: &str,
    ) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO access_log (vault_slug, email, source, path) VALUES (?, ?, ?, ?)")
            .bind(slug)
            .bind(email)
            .bind(source)
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Per-day hit counts split by source, newest day first, capped at the
    /// 90-day retention window (LIMIT 90).
    pub async fn stats_per_day(&self, slug: &str) -> anyhow::Result<Vec<(String, String, i64)>> {
        Ok(sqlx::query_as(
            "SELECT date(ts) d, source, COUNT(*) FROM access_log \
             WHERE vault_slug = ? GROUP BY d, source ORDER BY d DESC LIMIT 90",
        )
        .bind(slug)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Top 10 most-hit web paths for a vault (mcp hits excluded).
    pub async fn stats_top_pages(&self, slug: &str) -> anyhow::Result<Vec<(String, i64)>> {
        Ok(sqlx::query_as(
            "SELECT path, COUNT(*) FROM access_log \
             WHERE vault_slug = ? AND source = 'web' \
             GROUP BY path ORDER BY COUNT(*) DESC LIMIT 10",
        )
        .bind(slug)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Total hits by one sharee on one vault, across all sources.
    pub async fn stats_own_count(&self, slug: &str, email: &str) -> anyhow::Result<i64> {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM access_log WHERE vault_slug = ? AND email = ?")
                .bind(slug)
                .bind(email)
                .fetch_one(&self.pool)
                .await?;
        Ok(n)
    }

    /// 90-day retention sweep (REQ-VLT-011): delete rows older than 90 days,
    /// return the deleted count. Called once at boot from `main::serve`.
    pub async fn prune_access_log(&self) -> anyhow::Result<u64> {
        let r = sqlx::query("DELETE FROM access_log WHERE ts < datetime('now', '-90 days')")
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    fn unix_now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_secs()
    }

    // ─── API tokens (Phase 10, PLAN 10-01) ───────────────────────────────

    /// Create a named API token for `user_id`. Returns `(id, plaintext)` —
    /// the plaintext is the ONLY copy (sha256 hex is what lands in the DB),
    /// same convention as invitations/reset_tokens.
    pub async fn create_api_token(
        &self,
        user_id: i64,
        name: &str,
    ) -> anyhow::Result<(i64, String)> {
        use sha2::{Digest, Sha256};
        let plain = format!("mn_{}", uuid::Uuid::new_v4().simple());
        let mut h = Sha256::new();
        h.update(plain.as_bytes());
        let hash = hex::encode(h.finalize());
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO api_tokens(user_id, name, token_hash, created_at) VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(user_id)
        .bind(name)
        .bind(&hash)
        .bind(self.unix_now() as i64)
        .fetch_one(&self.pool)
        .await?;
        Ok((id, plain))
    }

    /// The user's tokens, newest first — never exposes the hash.
    pub async fn list_api_tokens(&self, user_id: i64) -> anyhow::Result<Vec<ApiToken>> {
        let rows: Vec<(i64, String, i64, Option<i64>)> = sqlx::query_as(
            "SELECT id, name, created_at, last_used_at FROM api_tokens WHERE user_id = ? ORDER BY created_at DESC, id DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, created_at, last_used_at)| ApiToken {
                id,
                name,
                created_at,
                last_used_at,
            })
            .collect())
    }

    /// The user id behind an email (claims carry the email, tokens key on id).
    pub async fn user_id_for(&self, email: &str) -> anyhow::Result<Option<i64>> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(id,)| id))
    }

    /// Revoke one of the user's own tokens. `false` if the id is unknown OR
    /// belongs to another user (no cross-user revoke).
    pub async fn revoke_api_token(&self, user_id: i64, id: i64) -> anyhow::Result<bool> {
        let res = sqlx::query("DELETE FROM api_tokens WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() == 1)
    }

    /// Resolve a plaintext bearer token to its user, stamping `last_used_at`.
    /// `None` for unknown/revoked tokens.
    pub async fn user_for_api_token(&self, plain: &str) -> anyhow::Result<Option<User>> {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(plain.as_bytes());
        let hash = hex::encode(h.finalize());
        let row: Option<(i64, i64)> =
            sqlx::query_as("SELECT id, user_id FROM api_tokens WHERE token_hash = ?")
                .bind(&hash)
                .fetch_optional(&self.pool)
                .await?;
        let Some((token_id, user_id)) = row else {
            return Ok(None);
        };
        let now = self.unix_now() as i64;
        sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE id = ?")
            .bind(now)
            .bind(token_id)
            .execute(&self.pool)
            .await?;
        let user = sqlx::query_as::<_, (i64, String, String, Option<String>)>(
            "SELECT id, email, role, display_name FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(user.map(|(id, email, role, display_name)| User {
            id,
            email,
            role,
            display_name,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> Store {
        Store::connect_memory().await.expect("memory store")
    }

    #[tokio::test]
    async fn migrations_create_tables() {
        let s = store().await;
        // Both migrations ran: querying the tables succeeds.
        let _: Vec<(i64,)> = sqlx::query_as("SELECT id FROM users")
            .fetch_all(&s.pool)
            .await
            .unwrap();
        let _: Vec<(i64,)> = sqlx::query_as("SELECT id FROM vaults")
            .fetch_all(&s.pool)
            .await
            .unwrap();
        let _: Vec<(i64,)> = sqlx::query_as("SELECT vault_id, user_id FROM vault_acl")
            .fetch_all(&s.pool)
            .await
            .unwrap();
        let _: Vec<(i64,)> = sqlx::query_as("SELECT id FROM invitations")
            .fetch_all(&s.pool)
            .await
            .unwrap();
        let _: Vec<(i64,)> = sqlx::query_as("SELECT id FROM reset_tokens")
            .fetch_all(&s.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn invitation_is_valid_until_consumed() {
        let s = store().await;
        s.create_invitation("hash-abc", "invited@x.fr", "reader", 86_400)
            .await
            .unwrap();
        let (email, role) = s
            .validate_invitation("hash-abc")
            .await
            .unwrap()
            .expect("fresh invitation is valid");
        assert_eq!(email, "invited@x.fr");
        assert_eq!(role, "reader");
    }

    #[tokio::test]
    async fn unknown_or_expired_invitation_is_rejected() {
        let s = store().await;
        // Never issued.
        assert!(s.validate_invitation("nope").await.unwrap().is_none());
        // Issued with a zero TTL → expires at the instant it was created.
        s.create_invitation("hash-expired", "late@x.fr", "reader", 0)
            .await
            .unwrap();
        assert!(
            s.validate_invitation("hash-expired")
                .await
                .unwrap()
                .is_none(),
            "expired invitation must not validate"
        );
    }

    #[tokio::test]
    async fn consuming_an_invitation_creates_the_user_with_its_role() {
        let s = store().await;
        s.create_invitation("hash-use", "newbie@x.fr", "reader", 86_400)
            .await
            .unwrap();
        let email = s
            .consume_invitation("hash-use", "Ch0osen!pw")
            .await
            .unwrap()
            .expect("invitation consumes");
        assert_eq!(email, "newbie@x.fr");
        assert_eq!(
            s.role_for("newbie@x.fr").await.unwrap().as_deref(),
            Some("reader")
        );
        // The chosen password works.
        assert!(s
            .verify_login("newbie@x.fr", "Ch0osen!pw")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn invitation_is_single_use() {
        let s = store().await;
        s.create_invitation("hash-1x", "once@x.fr", "reader", 86_400)
            .await
            .unwrap();
        assert!(s
            .consume_invitation("hash-1x", "pw1")
            .await
            .unwrap()
            .is_some());
        // Second attempt: no user creation, no error — just refused.
        assert!(s
            .consume_invitation("hash-1x", "pw2")
            .await
            .unwrap()
            .is_none());
        let users = s.list_users().await.unwrap();
        assert_eq!(
            users.len(),
            1,
            "a replayed invitation must not create a second user"
        );
        // The password from the FIRST consumption stands.
        assert!(s.verify_login("once@x.fr", "pw1").await.unwrap().is_some());
        assert!(s.verify_login("once@x.fr", "pw2").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn invitation_plaintext_token_is_never_stored() {
        let s = store().await;
        s.create_invitation("sha256-of-the-secret", "invited@x.fr", "reader", 86_400)
            .await
            .unwrap();
        let stored: Vec<(String,)> = sqlx::query_as("SELECT token_hash FROM invitations")
            .fetch_all(&s.pool)
            .await
            .unwrap();
        assert_eq!(stored, vec![("sha256-of-the-secret".to_string(),)]);
    }

    #[tokio::test]
    async fn reset_token_is_valid_and_resolves_its_user() {
        let s = store().await;
        s.create_user("reset@x.fr", "old-pw", "reader")
            .await
            .unwrap();
        s.create_reset_token("hash-rst", "reset@x.fr", 3600)
            .await
            .unwrap();
        let user_id = s
            .validate_reset_token("hash-rst")
            .await
            .unwrap()
            .expect("fresh reset token is valid");
        let expected_id = s
            .list_users()
            .await
            .unwrap()
            .into_iter()
            .find(|u| u.email == "reset@x.fr")
            .expect("user exists")
            .id;
        assert_eq!(user_id, expected_id);
    }

    #[tokio::test]
    async fn unknown_email_gets_no_reset_token() {
        let s = store().await;
        // Asking for a reset on an unknown address must be a silent no-op,
        // never an error and never a stored token (no account enumeration).
        s.create_reset_token("hash-ghost", "ghost@x.fr", 3600)
            .await
            .expect("unknown email does not error");
        assert!(s
            .validate_reset_token("hash-ghost")
            .await
            .unwrap()
            .is_none());
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM reset_tokens")
            .fetch_one(&s.pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0, "no token row must exist for an unknown email");
    }

    #[tokio::test]
    async fn unknown_or_expired_reset_token_is_rejected() {
        let s = store().await;
        s.create_user("reset@x.fr", "old-pw", "reader")
            .await
            .unwrap();
        assert!(s.validate_reset_token("nope").await.unwrap().is_none());
        s.create_reset_token("hash-rst-exp", "reset@x.fr", 0)
            .await
            .unwrap();
        assert!(
            s.validate_reset_token("hash-rst-exp")
                .await
                .unwrap()
                .is_none(),
            "expired reset token must not validate"
        );
    }

    #[tokio::test]
    async fn consuming_a_reset_token_replaces_the_password() {
        let s = store().await;
        s.create_user("reset@x.fr", "old-pw", "reader")
            .await
            .unwrap();
        s.create_reset_token("hash-rst-use", "reset@x.fr", 3600)
            .await
            .unwrap();
        let user_id = s
            .consume_reset_token("hash-rst-use", "N3w!pw")
            .await
            .unwrap()
            .expect("reset consumes");
        assert!(user_id > 0);
        // New password works, old one is dead.
        assert!(s
            .verify_login("reset@x.fr", "N3w!pw")
            .await
            .unwrap()
            .is_some());
        assert!(s
            .verify_login("reset@x.fr", "old-pw")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn reset_token_is_single_use() {
        let s = store().await;
        s.create_user("reset@x.fr", "old-pw", "reader")
            .await
            .unwrap();
        s.create_reset_token("hash-rst-1x", "reset@x.fr", 3600)
            .await
            .unwrap();
        assert!(s
            .consume_reset_token("hash-rst-1x", "first-pw")
            .await
            .unwrap()
            .is_some());
        // Replay refused: the first password stands, the replayed one is ignored.
        assert!(s
            .consume_reset_token("hash-rst-1x", "attacker-pw")
            .await
            .unwrap()
            .is_none());
        assert!(s
            .verify_login("reset@x.fr", "first-pw")
            .await
            .unwrap()
            .is_some());
        assert!(s
            .verify_login("reset@x.fr", "attacker-pw")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn ensure_admin_is_idempotent() {
        let s = store().await;
        s.ensure_admin("admin@x.fr", "pw1").await.unwrap();
        s.ensure_admin("admin@x.fr", "pw1").await.unwrap();
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&s.pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn verify_login_checks_bcrypt_password() {
        let s = store().await;
        s.ensure_admin("admin@x.fr", "S3cret!").await.unwrap();
        let user = s
            .verify_login("admin@x.fr", "S3cret!")
            .await
            .unwrap()
            .expect("valid login");
        assert_eq!(user.role, "admin");
        assert!(s
            .verify_login("admin@x.fr", "WRONG")
            .await
            .unwrap()
            .is_none());
        assert!(s
            .verify_login("ghost@x.fr", "S3cret!")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn admin_sees_all_vaults() {
        let s = store().await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.seed_vault("demo", "Demo", Path::new("/tmp/demo-book"))
            .await
            .unwrap();
        s.seed_vault("second", "Second", Path::new("/tmp/second-book"))
            .await
            .unwrap();
        let vaults = s.list_vaults_for("admin@x.fr").await.unwrap();
        assert_eq!(vaults.len(), 2);
        assert_eq!(vaults[0].slug, "demo");
    }

    #[tokio::test]
    async fn reader_sees_only_acl_granted_vaults() {
        let s = store().await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.seed_vault("demo", "Demo", Path::new("/tmp/demo-book"))
            .await
            .unwrap();
        s.create_user("reader@x.fr", "pw", "reader").await.unwrap();

        // No ACL yet → nothing visible, access denied.
        assert!(s.list_vaults_for("reader@x.fr").await.unwrap().is_empty());
        assert!(!s.acl_allows("reader@x.fr", "demo").await.unwrap());

        // Grant ACL → visible + allowed.
        s.grant_acl("reader@x.fr", "demo").await.unwrap();
        let vaults = s.list_vaults_for("reader@x.fr").await.unwrap();
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].slug, "demo");
        assert!(s.acl_allows("reader@x.fr", "demo").await.unwrap());
    }

    #[tokio::test]
    async fn unknown_user_is_denied() {
        let s = store().await;
        s.seed_vault("demo", "Demo", Path::new("/tmp/demo-book"))
            .await
            .unwrap();
        assert!(!s.acl_allows("nobody@x.fr", "demo").await.unwrap());
        assert!(s.list_vaults_for("nobody@x.fr").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn seed_vault_updates_changed_path() {
        let s = store().await;
        s.seed_vault("demo", "Demo", Path::new("/tmp/old"))
            .await
            .unwrap();
        s.seed_vault("demo", "Demo v2", Path::new("/tmp/new"))
            .await
            .unwrap();
        let vaults = s.list_all_vaults().await.unwrap();
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].book_path, "/tmp/new");
        assert_eq!(vaults[0].name, "Demo v2");
    }

    // ---------- Phase 10.1 / PLAN 10.1-01 Task 1: migration 0008 shape ----------

    #[tokio::test]
    async fn shares_stats_migration_shape() {
        let s = store().await;
        s.seed_vault("shape", "Shape", Path::new("/tmp/shape/book"))
            .await
            .unwrap();
        // New vaults columns: allow_reshare defaults to 0, last_refreshed_at
        // is stamped by seed_vault.
        let (allow, refreshed): (i64, Option<String>) = sqlx::query_as(
            "SELECT allow_reshare, last_refreshed_at FROM vaults WHERE slug = 'shape'",
        )
        .fetch_one(&s.pool)
        .await
        .unwrap();
        assert_eq!(allow, 0, "allow_reshare defaults to 0");
        assert!(refreshed.is_some(), "seed_vault stamps last_refreshed_at");
        // Both new tables exist and are queryable.
        let _: Vec<(i64,)> = sqlx::query_as("SELECT vault_id FROM vault_shares")
            .fetch_all(&s.pool)
            .await
            .unwrap();
        let _: Vec<(i64,)> = sqlx::query_as("SELECT id FROM access_log")
            .fetch_all(&s.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn vault_shares_rejects_bad_role_and_duplicate_pair() {
        let s = store().await;
        s.seed_vault("v", "V", Path::new("/tmp/v/book"))
            .await
            .unwrap();
        let bad = sqlx::query(
            "INSERT INTO vault_shares (vault_id, email, role) \
             SELECT id, 'a@x.fr', 'admin' FROM vaults WHERE slug = 'v'",
        )
        .execute(&s.pool)
        .await;
        assert!(bad.is_err(), "role CHECK must reject 'admin'");
        sqlx::query(
            "INSERT INTO vault_shares (vault_id, email, role) \
             SELECT id, 'a@x.fr', 'reader' FROM vaults WHERE slug = 'v'",
        )
        .execute(&s.pool)
        .await
        .unwrap();
        let dup = sqlx::query(
            "INSERT INTO vault_shares (vault_id, email, role) \
             SELECT id, 'a@x.fr', 'writer' FROM vaults WHERE slug = 'v'",
        )
        .execute(&s.pool)
        .await;
        assert!(dup.is_err(), "duplicate (vault_id, email) must be rejected");
    }

    #[tokio::test]
    async fn access_log_rejects_bad_source() {
        let s = store().await;
        let bad = sqlx::query(
            "INSERT INTO access_log (vault_slug, email, source, path) \
             VALUES ('v', 'a@x.fr', 'ftp', '/x')",
        )
        .execute(&s.pool)
        .await;
        assert!(bad.is_err(), "source CHECK must reject 'ftp'");
        sqlx::query(
            "INSERT INTO access_log (vault_slug, email, source, path) \
             VALUES ('v', 'a@x.fr', 'web', '/x')",
        )
        .execute(&s.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO access_log (vault_slug, email, source, path) \
             VALUES ('v', 'a@x.fr', 'mcp', '/y')",
        )
        .execute(&s.pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn delete_vault_cascades_share_rows() {
        let s = store().await;
        s.seed_vault("gone", "Gone", Path::new("/tmp/gone/book"))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO vault_shares (vault_id, email, role) \
             SELECT id, 'a@x.fr', 'reader' FROM vaults WHERE slug = 'gone'",
        )
        .execute(&s.pool)
        .await
        .unwrap();
        s.delete_vault("gone").await.unwrap();
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM vault_shares")
            .fetch_one(&s.pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "vault_shares rows cascade with the vault");
    }

    #[tokio::test]
    async fn seed_vault_stamps_last_refreshed_at() {
        let s = store().await;
        s.seed_vault("stamp", "Stamp", Path::new("/tmp/stamp/book"))
            .await
            .unwrap();
        let (t1,): (Option<String>,) =
            sqlx::query_as("SELECT last_refreshed_at FROM vaults WHERE slug = 'stamp'")
                .fetch_one(&s.pool)
                .await
                .unwrap();
        let t1 = t1.expect("stamped on insert");
        // Same path again: the no-op branch must not restamp (idempotent).
        s.seed_vault("stamp", "Stamp", Path::new("/tmp/stamp/book"))
            .await
            .unwrap();
        let (t2,): (String,) =
            sqlx::query_as("SELECT last_refreshed_at FROM vaults WHERE slug = 'stamp'")
                .fetch_one(&s.pool)
                .await
                .unwrap();
        assert_eq!(t1, t2, "unchanged path must not restamp");
        // Changed path: the UPDATE branch restamps.
        sqlx::query(
            "UPDATE vaults SET last_refreshed_at = '2000-01-01 00:00:00' WHERE slug = 'stamp'",
        )
        .execute(&s.pool)
        .await
        .unwrap();
        s.seed_vault("stamp", "Stamp", Path::new("/tmp/stamp/other"))
            .await
            .unwrap();
        let (t3,): (String,) =
            sqlx::query_as("SELECT last_refreshed_at FROM vaults WHERE slug = 'stamp'")
                .fetch_one(&s.pool)
                .await
                .unwrap();
        assert_ne!(t3, "2000-01-01 00:00:00", "changed path must restamp");
    }
}

#[cfg(test)]
mod access_tests {
    use super::*;

    /// Each store is in-memory, so one per test is enough — the `name` is only
    /// documentation of which fixture a test uses.
    async fn st(_name: &str) -> Store {
        Store::connect_memory().await.expect("store")
    }

    #[tokio::test]
    async fn existing_vaults_become_global_after_migration() {
        // A vault created by the legacy path has no owner and is global, so
        // nothing that worked before the migration stops working.
        let s = st("legacy").await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.seed_vault("legacy", "Legacy", Path::new("/tmp/legacy/book"))
            .await
            .unwrap();
        let (scope, owner): (String, Option<i64>) =
            sqlx::query_as("SELECT scope, owner_id FROM vaults WHERE slug = 'legacy'")
                .fetch_one(&s.pool)
                .await
                .unwrap();
        assert_eq!(scope, "global");
        assert_eq!(owner, None);
    }

    #[tokio::test]
    async fn groups_carry_members() {
        let s = st("groups").await;
        s.create_user("a@x.fr", "pw", "reader").await.unwrap();
        s.create_user("b@x.fr", "pw", "reader").await.unwrap();
        let g = s.create_group("Team").await.unwrap();
        assert!(s.add_group_member(g, "a@x.fr").await.unwrap());
        // Adding an unknown user is refused rather than silently ignored.
        assert!(!s.add_group_member(g, "ghost@x.fr").await.unwrap());
        assert_eq!(s.group_members(g).await.unwrap(), vec!["a@x.fr"]);
        assert!(s.remove_group_member(g, "a@x.fr").await.unwrap());
        assert!(s.group_members(g).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn deleting_a_group_cascades_its_memberships() {
        let s = st("cascade").await;
        s.create_user("a@x.fr", "pw", "reader").await.unwrap();
        let g = s.create_group("Temp").await.unwrap();
        s.add_group_member(g, "a@x.fr").await.unwrap();
        s.delete_group(g).await.unwrap();
        assert!(s.group_members(g).await.unwrap().is_empty());
        assert!(s.list_groups().await.unwrap().is_empty());
    }

    // ---------- personal vault quota (REQ-GRP-005..008, REQ-VLT-008) ----------

    #[tokio::test]
    async fn personal_vault_delete_is_owner_only_and_releases_the_quota() {
        // Without a delete, a `free` member could exhaust their 2 slots for
        // life. Deleting is owner-only: another user (even an admin-ish
        // sibling owner) cannot drop someone else's vault.
        let s = st("pv-delete").await;
        s.create_user("a@x.fr", "pw", "reader").await.unwrap();
        s.create_user("b@x.fr", "pw", "reader").await.unwrap();
        let g = s.create_group("free").await.unwrap();
        s.set_group_limit(g, Some(2)).await.unwrap();
        s.add_group_member(g, "a@x.fr").await.unwrap();
        s.add_group_member(g, "b@x.fr").await.unwrap();
        s.create_personal_vault("a@x.fr", "v1", "V1", Path::new("/tmp/d1/book"))
            .await
            .unwrap();
        s.create_personal_vault("a@x.fr", "v2", "V2", Path::new("/tmp/d2/book"))
            .await
            .unwrap();
        // Quota full: a third is refused.
        assert!(s
            .create_personal_vault("a@x.fr", "v3", "V3", Path::new("/tmp/d3/book"))
            .await
            .is_err());
        // B cannot delete A's vault.
        assert!(!s.delete_personal_vault("b@x.fr", "v1").await.unwrap());
        assert_eq!(s.count_personal_vaults("a@x.fr").await.unwrap(), 2);
        // A deletes v1 → the slot is free again.
        assert!(s.delete_personal_vault("a@x.fr", "v1").await.unwrap());
        assert_eq!(s.count_personal_vaults("a@x.fr").await.unwrap(), 1);
        s.create_personal_vault("a@x.fr", "v3", "V3", Path::new("/tmp/d3/book"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn vault_source_roundtrips() {
        // A loaded vault remembers where it came from so it can be refreshed.
        let s = st("pv-source").await;
        s.create_user("a@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault("a@x.fr", "v1", "V1", Path::new("/tmp/s1/book"))
            .await
            .unwrap();
        assert_eq!(s.vault_source("v1").await.unwrap(), None);
        s.set_vault_source("v1", "https://github.com/lejeunecedric/B3Wiki")
            .await
            .unwrap();
        assert_eq!(
            s.vault_source("v1").await.unwrap().as_deref(),
            Some("https://github.com/lejeunecedric/B3Wiki")
        );
    }

    #[tokio::test]
    async fn group_limit_caps_personal_vault_creation() {
        // REQ-VLT-008: a member of `free` (limit 2) creates 2 personal vaults,
        // the 3rd is refused with a clear error.
        let s = st("quota-cap").await;
        s.create_user("demo@x.fr", "pw", "reader").await.unwrap();
        let g = s.create_group("free").await.unwrap();
        s.set_group_limit(g, Some(2)).await.unwrap();
        s.add_group_member(g, "demo@x.fr").await.unwrap();
        s.create_personal_vault("demo@x.fr", "v1", "V1", Path::new("/tmp/q1/book"))
            .await
            .unwrap();
        s.create_personal_vault("demo@x.fr", "v2", "V2", Path::new("/tmp/q2/book"))
            .await
            .unwrap();
        let err = s
            .create_personal_vault("demo@x.fr", "v3", "V3", Path::new("/tmp/q3/book"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("limit"), "clear error: {err}");
        assert_eq!(s.count_personal_vaults("demo@x.fr").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn shared_vaults_do_not_count_toward_the_quota() {
        // REQ-VLT-008: the quota counts PERSONAL vaults only — shared/global
        // ones are granted by groups and never consume a member's quota.
        let s = st("quota-shared").await;
        s.create_user("demo@x.fr", "pw", "reader").await.unwrap();
        let g = s.create_group("free").await.unwrap();
        s.set_group_limit(g, Some(1)).await.unwrap();
        s.add_group_member(g, "demo@x.fr").await.unwrap();
        s.seed_vault("shared", "Shared", Path::new("/tmp/sh/book"))
            .await
            .unwrap();
        // The shared vault above does not eat the quota: one personal still fits.
        s.create_personal_vault("demo@x.fr", "v1", "V1", Path::new("/tmp/sq1/book"))
            .await
            .unwrap();
        assert!(s
            .create_personal_vault("demo@x.fr", "v2", "V2", Path::new("/tmp/sq2/book"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn effective_limit_is_the_most_permissive_group() {
        // REQ-GRP-008: NULL (unlimited) beats any number; otherwise the max
        // across the user's groups. No group at all = no configured limit.
        let s = st("quota-effective").await;
        s.create_user("a@x.fr", "pw", "reader").await.unwrap();
        s.create_user("b@x.fr", "pw", "reader").await.unwrap();
        s.create_user("c@x.fr", "pw", "reader").await.unwrap();
        s.create_user("lone@x.fr", "pw", "reader").await.unwrap();
        let free = s.create_group("free").await.unwrap();
        s.set_group_limit(free, Some(2)).await.unwrap();
        let power = s.create_group("power").await.unwrap();
        s.set_group_limit(power, Some(5)).await.unwrap();
        let unlimited = s.create_group("unlimited").await.unwrap();
        s.add_group_member(free, "a@x.fr").await.unwrap();
        s.add_group_member(free, "b@x.fr").await.unwrap();
        s.add_group_member(power, "b@x.fr").await.unwrap();
        s.add_group_member(unlimited, "c@x.fr").await.unwrap();
        assert_eq!(
            s.effective_personal_vault_limit("a@x.fr").await.unwrap(),
            Some(2)
        );
        assert_eq!(
            s.effective_personal_vault_limit("b@x.fr").await.unwrap(),
            Some(5)
        );
        assert_eq!(
            s.effective_personal_vault_limit("c@x.fr").await.unwrap(),
            None
        );
        assert_eq!(
            s.effective_personal_vault_limit("lone@x.fr").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn group_limit_roundtrips_through_list_groups() {
        let s = st("quota-list").await;
        let g = s.create_group("free").await.unwrap();
        s.set_group_limit(g, Some(2)).await.unwrap();
        assert_eq!(
            s.list_groups().await.unwrap(),
            vec![(g, "free".to_string(), 0, Some(2))]
        );
    }

    // ---------- builtin groups + default memberships (REQ-GRP-006/007) ----------

    #[tokio::test]
    async fn fallback_admin_appears_when_the_instance_has_no_admin() {
        // Anti-lockout: demoting the ONLY admin used to leave the instance
        // unmanageable (no /admin access at all). At boot, if no admin remains,
        // the well-known `admin`/`admin` account is created.
        let s = st("fallback-admin").await;
        s.ensure_builtin_groups().await.unwrap();
        s.create_user("demo", "demo", "reader").await.unwrap();
        s.ensure_fallback_admin().await.unwrap();
        assert_eq!(s.role_for("admin").await.unwrap().as_deref(), Some("admin"));
        assert!(
            s.verify_login("admin", "admin").await.unwrap().is_some(),
            "admin/admin must be able to log in"
        );
        assert_eq!(
            s.effective_personal_vault_limit("admin").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn fallback_admin_repromotes_a_demoted_admin_account() {
        let s = st("fallback-repromote").await;
        s.ensure_builtin_groups().await.unwrap();
        s.create_user("admin", "admin", "admin").await.unwrap();
        s.set_user_role("admin", "reader").await.unwrap();
        assert_eq!(
            s.role_for("admin").await.unwrap().as_deref(),
            Some("reader")
        );
        s.ensure_fallback_admin().await.unwrap();
        assert_eq!(s.role_for("admin").await.unwrap().as_deref(), Some("admin"));
        assert_eq!(
            s.effective_personal_vault_limit("admin").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn fallback_admin_leaves_a_healthy_instance_alone() {
        let s = st("fallback-noop").await;
        s.ensure_builtin_groups().await.unwrap();
        s.create_user("root@x.fr", "pw", "admin").await.unwrap();
        s.create_user("demo", "demo", "reader").await.unwrap();
        s.ensure_fallback_admin().await.unwrap();
        assert_eq!(s.role_for("demo").await.unwrap().as_deref(), Some("reader"));
        assert!(s.role_for("admin").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn switching_role_moves_the_user_to_the_right_plan() {
        // REQ-GRP-007: demoting `demo` to reader drops it out of `admin` and
        // lands it in `free` (quota 2); re-promoting grants the unlimited plan;
        // a user already placed in another group is left alone.
        let s = st("role-switch").await;
        s.ensure_builtin_groups().await.unwrap();
        s.create_user("demo@x.fr", "pw", "admin").await.unwrap();
        s.create_user("team@x.fr", "pw", "reader").await.unwrap();
        let team = s.create_group("team").await.unwrap();
        s.set_group_limit(team, Some(7)).await.unwrap();
        s.add_group_member(team, "team@x.fr").await.unwrap();

        s.set_user_role("demo@x.fr", "reader").await.unwrap();
        assert_eq!(
            s.role_for("demo@x.fr").await.unwrap().as_deref(),
            Some("reader")
        );
        assert_eq!(
            s.effective_personal_vault_limit("demo@x.fr").await.unwrap(),
            Some(2),
            "demoted user gets the free quota"
        );

        s.set_user_role("demo@x.fr", "admin").await.unwrap();
        assert_eq!(
            s.effective_personal_vault_limit("demo@x.fr").await.unwrap(),
            None
        );

        assert_eq!(
            s.effective_personal_vault_limit("team@x.fr").await.unwrap(),
            Some(7)
        );
    }

    #[tokio::test]
    async fn builtin_groups_seeded_with_default_limits() {
        // REQ-GRP-006: `free` (limit 2) and `admin` (unlimited) exist after the
        // seed, and seeding twice does not duplicate them.
        let s = st("builtin").await;
        s.ensure_builtin_groups().await.unwrap();
        s.ensure_builtin_groups().await.unwrap();
        let groups = s.list_groups().await.unwrap();
        let by_name = |n: &str| groups.iter().find(|(_, name, _, _)| name == n).cloned();
        assert_eq!(by_name("free").map(|(_, _, _, l)| l), Some(Some(2)));
        assert_eq!(by_name("admin").map(|(_, _, _, l)| l), Some(None));
        assert_eq!(groups.len(), 2);
    }

    #[tokio::test]
    async fn new_users_join_their_default_group() {
        // REQ-GRP-006/007: a reader joins `free` (quota 2), an admin joins
        // `admin` (unlimited) — as soon as the builtin groups are provisioned.
        let s = st("default-join").await;
        s.ensure_builtin_groups().await.unwrap();
        s.create_user("demo@x.fr", "pw", "reader").await.unwrap();
        s.create_user("root@x.fr", "pw", "admin").await.unwrap();
        assert_eq!(
            s.effective_personal_vault_limit("demo@x.fr").await.unwrap(),
            Some(2)
        );
        assert_eq!(
            s.effective_personal_vault_limit("root@x.fr").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn groupless_users_are_swept_into_their_default_group_at_boot() {
        // REQ-GRP-007: existing accounts (e.g. `demo`) land in `free` / `admin`
        // on boot — but a user already placed in another group is left alone.
        let s = st("sweep").await;
        s.create_user("demo@x.fr", "pw", "reader").await.unwrap();
        s.create_user("root@x.fr", "pw", "admin").await.unwrap();
        s.create_user("team@x.fr", "pw", "reader").await.unwrap();
        let team = s.create_group("team").await.unwrap();
        s.set_group_limit(team, Some(7)).await.unwrap();
        s.add_group_member(team, "team@x.fr").await.unwrap();
        s.ensure_builtin_groups().await.unwrap();
        s.ensure_default_memberships().await.unwrap();
        assert_eq!(
            s.effective_personal_vault_limit("demo@x.fr").await.unwrap(),
            Some(2)
        );
        assert_eq!(
            s.effective_personal_vault_limit("root@x.fr").await.unwrap(),
            None
        );
        assert_eq!(
            s.effective_personal_vault_limit("team@x.fr").await.unwrap(),
            Some(7)
        );
    }

    #[tokio::test]
    async fn personal_vault_is_invisible_to_other_users() {
        let s = st("personal").await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.create_user("alice@x.fr", "pw", "reader").await.unwrap();
        s.create_user("bob@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault(
            "alice@x.fr",
            "secret",
            "Alice's notes",
            Path::new("/tmp/alice/book"),
        )
        .await
        .unwrap();

        // Alice sees it, with write.
        let alice = s.accessible_vaults("alice@x.fr").await.unwrap();
        assert_eq!(alice.len(), 1);
        assert!(alice[0].personal);
        assert_eq!(alice[0].role, "write");

        // Bob does not see it at all — not in the list, and no access even when
        // he guesses the slug (REQ-VLT-002).
        assert!(s.accessible_vaults("bob@x.fr").await.unwrap().is_empty());
        assert_eq!(
            s.access_for("bob@x.fr", "secret").await.unwrap(),
            Access::None
        );
    }

    #[tokio::test]
    async fn global_vault_is_shared_through_a_group() {
        let s = st("global").await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.create_user("alice@x.fr", "pw", "reader").await.unwrap();
        s.create_user("bob@x.fr", "pw", "reader").await.unwrap();
        s.seed_vault("wiki", "Wiki", Path::new("/tmp/wiki/book"))
            .await
            .unwrap();

        let g = s.create_group("Readers").await.unwrap();
        s.add_group_member(g, "alice@x.fr").await.unwrap();
        s.set_vault_group_role("wiki", g, "read").await.unwrap();

        assert_eq!(
            s.access_for("alice@x.fr", "wiki").await.unwrap(),
            Access::Read
        );
        assert_eq!(
            s.access_for("bob@x.fr", "wiki").await.unwrap(),
            Access::None
        );

        let alice = s.accessible_vaults("alice@x.fr").await.unwrap();
        assert_eq!(alice.len(), 1);
        assert!(!alice[0].personal);
        assert_eq!(alice[0].role, "read");
    }

    #[tokio::test]
    async fn write_role_comes_from_the_strongest_group() {
        let s = st("strongest").await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.create_user("alice@x.fr", "pw", "reader").await.unwrap();
        s.seed_vault("team", "Team", Path::new("/tmp/team/book"))
            .await
            .unwrap();
        let readers = s.create_group("Readers").await.unwrap();
        let writers = s.create_group("Writers").await.unwrap();
        s.add_group_member(readers, "alice@x.fr").await.unwrap();
        s.add_group_member(writers, "alice@x.fr").await.unwrap();
        s.set_vault_group_role("team", readers, "read")
            .await
            .unwrap();
        s.set_vault_group_role("team", writers, "write")
            .await
            .unwrap();

        assert_eq!(
            s.access_for("alice@x.fr", "team").await.unwrap(),
            Access::Write
        );
        assert_eq!(
            s.accessible_vaults("alice@x.fr").await.unwrap()[0].role,
            "write",
            "the strongest role wins in the listing too"
        );
    }

    #[tokio::test]
    async fn revoking_membership_removes_access() {
        let s = st("revoke").await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.create_user("alice@x.fr", "pw", "reader").await.unwrap();
        s.seed_vault("wiki", "Wiki", Path::new("/tmp/wiki/book"))
            .await
            .unwrap();
        let g = s.create_group("Readers").await.unwrap();
        s.add_group_member(g, "alice@x.fr").await.unwrap();
        s.set_vault_group_role("wiki", g, "read").await.unwrap();
        assert!(s.access_for("alice@x.fr", "wiki").await.unwrap().can_read());

        s.remove_group_member(g, "alice@x.fr").await.unwrap();
        assert_eq!(
            s.access_for("alice@x.fr", "wiki").await.unwrap(),
            Access::None
        );
        assert!(s.accessible_vaults("alice@x.fr").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn admins_see_everything_including_personal_vaults() {
        let s = st("adminall").await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.create_user("alice@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault("alice@x.fr", "secret", "S", Path::new("/tmp/s/book"))
            .await
            .unwrap();
        s.seed_vault("shared", "Shared", Path::new("/tmp/sh/book"))
            .await
            .unwrap();

        let all = s.accessible_vaults("admin@x.fr").await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(
            s.access_for("admin@x.fr", "secret").await.unwrap(),
            Access::Write
        );
    }

    #[tokio::test]
    async fn unknown_user_and_unknown_vault_have_no_access() {
        let s = st("unknown").await;
        s.seed_vault("v", "V", Path::new("/tmp/v/book"))
            .await
            .unwrap();
        assert_eq!(s.access_for("ghost@x.fr", "v").await.unwrap(), Access::None);
        assert_eq!(
            s.access_for("admin@x.fr", "nope").await.unwrap(),
            Access::None
        );
        assert!(s.accessible_vaults("ghost@x.fr").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn display_name_round_trips_and_can_be_cleared() {
        let s = st("display").await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.set_display_name("admin@x.fr", "  Ada Lovelace  ")
            .await
            .unwrap();
        let u = s.verify_login("admin@x.fr", "pw").await.unwrap().unwrap();
        assert_eq!(u.display_name.as_deref(), Some("Ada Lovelace"));
        s.set_display_name("admin@x.fr", "   ").await.unwrap();
        let u = s.verify_login("admin@x.fr", "pw").await.unwrap().unwrap();
        assert_eq!(u.display_name, None, "blank clears the name");
    }

    // ─── Phase 10 / PLAN 10-01: API tokens ────────────────────────────────

    async fn user_id(s: &Store, email: &str) -> i64 {
        let (id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE email = ?")
            .bind(email)
            .fetch_one(&s.pool)
            .await
            .unwrap();
        id
    }

    #[tokio::test]
    async fn api_token_is_returned_once_and_stored_hashed() {
        let s = st("api-token").await;
        s.create_user("tok@x.fr", "pw", "reader").await.unwrap();
        let uid = user_id(&s, "tok@x.fr").await;

        let (id, plain) = s.create_api_token(uid, "laptop").await.unwrap();
        assert!(plain.starts_with("mn_"), "plaintext carries the mn_ prefix");

        // The DB holds ONLY the sha256 hex of the plaintext — never the plaintext.
        let (stored_hash,): (String,) =
            sqlx::query_as("SELECT token_hash FROM api_tokens WHERE id = ?")
                .bind(id)
                .fetch_one(&s.pool)
                .await
                .unwrap();
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(plain.as_bytes());
        assert_eq!(stored_hash, hex::encode(h.finalize()));
        assert!(!stored_hash.contains(&plain));

        // A second token for the same user is distinct.
        let (_, plain2) = s.create_api_token(uid, "phone").await.unwrap();
        assert_ne!(plain, plain2);
    }

    #[tokio::test]
    async fn api_token_resolves_user_and_records_last_used() {
        let s = st("api-token").await;
        s.create_user("tok@x.fr", "pw", "reader").await.unwrap();
        let uid = user_id(&s, "tok@x.fr").await;
        let (_, plain) = s.create_api_token(uid, "ci").await.unwrap();

        let u = s
            .user_for_api_token(&plain)
            .await
            .unwrap()
            .expect("valid token resolves");
        assert_eq!(u.email, "tok@x.fr");

        let (last_used,): (Option<i64>,) =
            sqlx::query_as("SELECT last_used_at FROM api_tokens WHERE user_id = ?")
                .bind(uid)
                .fetch_one(&s.pool)
                .await
                .unwrap();
        assert!(last_used.is_some(), "usage is recorded");
    }

    #[tokio::test]
    async fn unknown_or_revoked_api_token_is_rejected() {
        let s = st("api-token").await;
        s.create_user("tok@x.fr", "pw", "reader").await.unwrap();
        let uid = user_id(&s, "tok@x.fr").await;

        assert!(s
            .user_for_api_token("mn_never-issued")
            .await
            .unwrap()
            .is_none());

        let (id, plain) = s.create_api_token(uid, "doomed").await.unwrap();
        assert!(s.user_for_api_token(&plain).await.unwrap().is_some());
        assert!(
            s.revoke_api_token(uid, id).await.unwrap(),
            "owner can revoke"
        );
        assert!(
            s.user_for_api_token(&plain).await.unwrap().is_none(),
            "revoked token stops resolving"
        );
    }

    #[tokio::test]
    async fn revoke_is_scoped_to_the_owner() {
        let s = st("api-token").await;
        s.create_user("alice@x.fr", "pw", "reader").await.unwrap();
        s.create_user("bob@x.fr", "pw", "reader").await.unwrap();
        let alice = user_id(&s, "alice@x.fr").await;
        let bob = user_id(&s, "bob@x.fr").await;

        let (id, _) = s.create_api_token(alice, "mine").await.unwrap();
        assert!(
            !s.revoke_api_token(bob, id).await.unwrap(),
            "bob cannot revoke alice's token"
        );
        assert_eq!(s.list_api_tokens(alice).await.unwrap().len(), 1);
    }
}

#[cfg(test)]
mod share_tests {
    use super::*;

    async fn st(_name: &str) -> Store {
        Store::connect_memory().await.expect("store")
    }

    #[tokio::test]
    async fn share_vault_upserts_role_and_unhides() {
        let s = st("share-upsert").await;
        s.create_user("owner@x.fr", "pw", "reader").await.unwrap();
        s.create_user("b@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault(
            "owner@x.fr",
            "book",
            "Book",
            Path::new("/tmp/share-up/book"),
        )
        .await
        .unwrap();

        s.share_vault("book", "b@x.fr", "reader").await.unwrap();
        let shares = s.list_shares("book").await.unwrap();
        assert_eq!(shares.len(), 1);
        assert_eq!(
            (shares[0].0.as_str(), shares[0].1.as_str()),
            ("b@x.fr", "reader")
        );
        assert!(!shares[0].2, "fresh share is not hidden");

        // Hidden, then re-shared with a new role: role updates AND hidden clears.
        assert!(s.hide_share("book", "b@x.fr").await.unwrap());
        s.share_vault("book", "b@x.fr", "writer").await.unwrap();
        let shares = s.list_shares("book").await.unwrap();
        assert_eq!(shares.len(), 1, "upsert must not duplicate the row");
        assert_eq!(shares[0].1, "writer");
        assert!(!shares[0].2, "re-sharing clears the hidden flag");

        // Guards: unknown slug and unknown role are refused.
        assert!(s.share_vault("nope", "b@x.fr", "reader").await.is_err());
        assert!(s.share_vault("book", "b@x.fr", "admin").await.is_err());
    }

    #[tokio::test]
    async fn revoke_and_hide_share_behave_differently() {
        let s = st("share-revoke-hide").await;
        s.create_user("owner@x.fr", "pw", "reader").await.unwrap();
        s.create_user("b@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault(
            "owner@x.fr",
            "book",
            "Book",
            Path::new("/tmp/share-rh/book"),
        )
        .await
        .unwrap();
        s.share_vault("book", "b@x.fr", "reader").await.unwrap();

        // hide: the row survives with hidden=1 (owner's list still shows it).
        assert!(s.hide_share("book", "b@x.fr").await.unwrap());
        let shares = s.list_shares("book").await.unwrap();
        assert_eq!(shares.len(), 1, "hidden share stays in the owner's list");
        assert!(shares[0].2, "hidden flag set");

        // revoke: the row is gone.
        assert!(s.revoke_share("book", "b@x.fr").await.unwrap());
        assert!(s.list_shares("book").await.unwrap().is_empty());
        assert!(
            !s.revoke_share("book", "b@x.fr").await.unwrap(),
            "second revoke is a no-op"
        );
        assert!(
            !s.hide_share("book", "b@x.fr").await.unwrap(),
            "hide on missing row is false"
        );
    }

    #[tokio::test]
    async fn share_reshare_matrix() {
        let s = st("reshare-matrix").await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.create_user("owner@x.fr", "pw", "reader").await.unwrap();
        s.create_user("reader@x.fr", "pw", "reader").await.unwrap();
        s.create_user("writer@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault(
            "owner@x.fr",
            "book",
            "Book",
            Path::new("/tmp/share-mx/book"),
        )
        .await
        .unwrap();
        s.share_vault("book", "reader@x.fr", "reader")
            .await
            .unwrap();
        s.share_vault("book", "writer@x.fr", "writer")
            .await
            .unwrap();

        // Owner and admin can always share.
        assert!(s.can_share("owner@x.fr", "book").await.unwrap());
        assert!(s.can_share("admin@x.fr", "book").await.unwrap());
        // Sharees cannot re-share while the vault toggle is off.
        assert!(!s.can_share("reader@x.fr", "book").await.unwrap());
        assert!(!s.can_share("writer@x.fr", "book").await.unwrap());

        // Toggle on: both sharees may re-share.
        s.set_allow_reshare("book", true).await.unwrap();
        assert!(s.can_share("reader@x.fr", "book").await.unwrap());
        assert!(s.can_share("writer@x.fr", "book").await.unwrap());

        // A hidden share does not grant re-share rights.
        s.hide_share("book", "writer@x.fr").await.unwrap();
        assert!(!s.can_share("writer@x.fr", "book").await.unwrap());
        s.hide_share("book", "writer@x.fr").await.unwrap(); // no-op, still hidden
        s.share_vault("book", "writer@x.fr", "writer")
            .await
            .unwrap(); // un-hide

        // Unknown email and revoked sharee: never.
        assert!(!s.can_share("ghost@x.fr", "book").await.unwrap());
        s.revoke_share("book", "reader@x.fr").await.unwrap();
        assert!(!s.can_share("reader@x.fr", "book").await.unwrap());

        // Toggle off again: sharees lose re-share rights.
        s.set_allow_reshare("book", false).await.unwrap();
        assert!(!s.can_share("writer@x.fr", "book").await.unwrap());
        assert!(s.can_share("owner@x.fr", "book").await.unwrap());
    }

    #[tokio::test]
    async fn rename_vault_keeps_slug() {
        let s = st("rename").await;
        s.seed_vault("book", "Old name", Path::new("/tmp/rename/book"))
            .await
            .unwrap();
        s.rename_vault("book", "New name").await.unwrap();
        assert_eq!(
            s.vault_name("book").await.unwrap().as_deref(),
            Some("New name")
        );
        let slugs: Vec<String> = s
            .list_all_vaults()
            .await
            .unwrap()
            .into_iter()
            .map(|v| v.slug)
            .collect();
        assert_eq!(
            slugs,
            vec!["book".to_string()],
            "slug is deliberately unchanged"
        );
    }

    #[tokio::test]
    async fn user_exists_reports_registered_accounts() {
        let s = st("user-exists").await;
        s.create_user("real@x.fr", "pw", "reader").await.unwrap();
        assert!(s.user_exists("real@x.fr").await.unwrap());
        assert!(!s.user_exists("ghost@x.fr").await.unwrap());
    }

    #[tokio::test]
    async fn grant_vault_to_all_groups_grants_read_to_every_group_and_is_idempotent() {
        let s = st("grant-all").await;
        s.seed_vault("docs", "Docs", Path::new("/tmp/grant-all/book"))
            .await
            .unwrap();
        let g1 = s.create_group("free").await.unwrap();
        let g2 = s.create_group("team").await.unwrap();
        let g3 = s.create_group("power").await.unwrap();
        s.set_vault_group_role("docs", g3, "write").await.unwrap();

        s.grant_vault_to_all_groups("docs").await.unwrap();
        let mut roles = s.vault_group_roles("docs").await.unwrap();
        roles.sort();
        assert_eq!(roles.len(), 3);
        assert!(roles.iter().any(|(id, _, r)| *id == g1 && r == "read"));
        assert!(roles.iter().any(|(id, _, r)| *id == g2 && r == "read"));
        assert!(
            roles.iter().any(|(id, _, r)| *id == g3 && r == "write"),
            "an existing stronger role is never downgraded"
        );

        // Idempotent: running the sweep again changes nothing.
        s.grant_vault_to_all_groups("docs").await.unwrap();
        assert_eq!(s.vault_group_roles("docs").await.unwrap().len(), 3);
    }

    // ---------- Task 3: ACL union — shares feed access_for/accessible_vaults ----------

    #[tokio::test]
    async fn share_grants_read_and_writer_grants_write() {
        let s = st("share-acl").await;
        s.ensure_admin("admin@x.fr", "pw").await.unwrap();
        s.create_user("owner@x.fr", "pw", "reader").await.unwrap();
        s.create_user("r@x.fr", "pw", "reader").await.unwrap();
        s.create_user("w@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault("owner@x.fr", "book", "Book", Path::new("/tmp/acl-rw/book"))
            .await
            .unwrap();
        s.share_vault("book", "r@x.fr", "reader").await.unwrap();
        s.share_vault("book", "w@x.fr", "writer").await.unwrap();

        assert_eq!(s.access_for("r@x.fr", "book").await.unwrap(), Access::Read);
        assert_eq!(s.access_for("w@x.fr", "book").await.unwrap(), Access::Write);

        let r = s.accessible_vaults("r@x.fr").await.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].vault.slug, "book");
        assert_eq!(r[0].role, "read");
        assert!(!r[0].personal);
        assert!(r[0].direct_share, "share-derived rows carry direct_share");

        let w = s.accessible_vaults("w@x.fr").await.unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].role, "write");
        assert!(w[0].direct_share);

        // Non-share rows never carry the flag.
        let owner = s.accessible_vaults("owner@x.fr").await.unwrap();
        assert_eq!(owner.len(), 1);
        assert!(!owner[0].direct_share, "own personal vault is not a share");
        let admin = s.accessible_vaults("admin@x.fr").await.unwrap();
        assert!(
            admin.iter().all(|av| !av.direct_share),
            "admin rows are not shares"
        );
    }

    #[tokio::test]
    async fn hidden_share_invisible_but_row_survives() {
        let s = st("share-hidden").await;
        s.create_user("owner@x.fr", "pw", "reader").await.unwrap();
        s.create_user("r@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault("owner@x.fr", "book", "Book", Path::new("/tmp/acl-hid/book"))
            .await
            .unwrap();
        s.share_vault("book", "r@x.fr", "reader").await.unwrap();
        assert_eq!(s.access_for("r@x.fr", "book").await.unwrap(), Access::Read);

        assert!(s.hide_share("book", "r@x.fr").await.unwrap());
        assert!(s.accessible_vaults("r@x.fr").await.unwrap().is_empty());
        assert_eq!(s.access_for("r@x.fr", "book").await.unwrap(), Access::None);

        // The owner's share list is unchanged — the row survives with hidden=1.
        let shares = s.list_shares("book").await.unwrap();
        assert_eq!(shares.len(), 1);
        assert!(shares[0].2);
    }

    #[tokio::test]
    async fn share_writer_yields_access_write_for_webdav_and_refresh() {
        let s = st("share-write").await;
        s.create_user("owner@x.fr", "pw", "reader").await.unwrap();
        s.create_user("w@x.fr", "pw", "reader").await.unwrap();
        s.create_user("r@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault("owner@x.fr", "book", "Book", Path::new("/tmp/acl-wd/book"))
            .await
            .unwrap();
        s.share_vault("book", "w@x.fr", "writer").await.unwrap();
        s.share_vault("book", "r@x.fr", "reader").await.unwrap();

        // WebDAV PUT and Refresh gate on can_write() — a writer share passes.
        assert!(s.access_for("w@x.fr", "book").await.unwrap().can_write());
        assert!(s.access_for("w@x.fr", "book").await.unwrap().can_read());
        // A reader share reads but never writes.
        assert!(s.access_for("r@x.fr", "book").await.unwrap().can_read());
        assert!(!s.access_for("r@x.fr", "book").await.unwrap().can_write());
    }

    #[tokio::test]
    async fn accessible_vaults_union_dedupes_group_and_share() {
        let s = st("share-union").await;
        s.create_user("alice@x.fr", "pw", "reader").await.unwrap();
        s.seed_vault("wiki", "Wiki", Path::new("/tmp/acl-union/book"))
            .await
            .unwrap();
        let g = s.create_group("Readers").await.unwrap();
        s.add_group_member(g, "alice@x.fr").await.unwrap();
        s.set_vault_group_role("wiki", g, "read").await.unwrap();
        // Same vault ALSO shared directly with the stronger writer role.
        s.share_vault("wiki", "alice@x.fr", "writer").await.unwrap();

        let alice = s.accessible_vaults("alice@x.fr").await.unwrap();
        assert_eq!(alice.len(), 1, "granted both ways: appears exactly once");
        assert_eq!(alice[0].role, "write", "strongest role wins");
        assert_eq!(
            s.access_for("alice@x.fr", "wiki").await.unwrap(),
            Access::Write
        );
    }

    #[tokio::test]
    async fn share_resolves_when_user_registers_later() {
        let s = st("share-pending").await;
        s.create_user("owner@x.fr", "pw", "reader").await.unwrap();
        s.create_personal_vault(
            "owner@x.fr",
            "book",
            "Book",
            Path::new("/tmp/acl-late/book"),
        )
        .await
        .unwrap();
        // Pending share: the email is not registered yet.
        s.share_vault("book", "late@x.fr", "reader").await.unwrap();
        assert!(s.accessible_vaults("late@x.fr").await.unwrap().is_empty());
        assert_eq!(
            s.access_for("late@x.fr", "book").await.unwrap(),
            Access::None
        );

        // Once the user exists, the share resolves with no further action.
        s.create_user("late@x.fr", "pw", "reader").await.unwrap();
        assert_eq!(
            s.access_for("late@x.fr", "book").await.unwrap(),
            Access::Read
        );
        let late = s.accessible_vaults("late@x.fr").await.unwrap();
        assert_eq!(late.len(), 1);
        assert_eq!(late[0].vault.slug, "book");
    }
}

// ---------- Task 4: access_log insert, aggregation, 90-day prune ----------

#[cfg(test)]
mod access_log_tests {
    use super::*;

    async fn st(_name: &str) -> Store {
        Store::connect_memory().await.expect("store")
    }

    #[tokio::test]
    async fn access_log_roundtrip_and_aggregation() {
        let s = st("access-log").await;

        // Fresh rows via log_access (today, datetime('now') default).
        s.log_access("a@x.fr", "wiki", "web", "/index.html")
            .await
            .unwrap();
        s.log_access("a@x.fr", "wiki", "web", "/index.html")
            .await
            .unwrap();
        s.log_access("a@x.fr", "wiki", "web", "/setup.html")
            .await
            .unwrap();
        s.log_access("b@x.fr", "wiki", "mcp", "/index.html")
            .await
            .unwrap();

        // Yesterday's rows inserted directly with an explicit ts.
        for (email, source, path) in [
            ("a@x.fr", "web", "/index.html"),
            ("a@x.fr", "web", "/old.html"),
            ("b@x.fr", "mcp", "/index.html"),
        ] {
            sqlx::query(
                "INSERT INTO access_log (vault_slug, email, source, path, ts)  VALUES ('wiki', ?, ?, ?, datetime('now', '-1 day'))",
            )
            .bind(email)
            .bind(source)
            .bind(path)
            .execute(&s.pool)
            .await
            .unwrap();
        }

        // Per-day grouping: two days, each split by source, newest first.
        let per_day = s.stats_per_day("wiki").await.unwrap();
        assert_eq!(per_day.len(), 4, "2 days x sources present, grouped");
        let today: (String,) = sqlx::query_as("SELECT date('now')")
            .fetch_one(&s.pool)
            .await
            .unwrap();
        let today = today.0;
        assert_eq!(per_day[0].0, today, "newest day first");
        let mut today_rows: Vec<(String, i64)> = per_day
            .iter()
            .filter(|r| r.0 == today)
            .map(|r| (r.1.clone(), r.2))
            .collect();
        today_rows.sort();
        assert_eq!(
            today_rows,
            vec![("mcp".to_string(), 1), ("web".to_string(), 3)]
        );
        let yesterday_rows: Vec<(String, i64)> = per_day
            .iter()
            .filter(|r| r.0 != today)
            .map(|r| (r.1.clone(), r.2))
            .collect();
        assert_eq!(yesterday_rows.len(), 2, "yesterday: web + mcp groups");

        // Top pages: web-only, most-hit first. /index.html has 3 web hits,
        // /setup.html 1, /old.html 1 — the mcp /index.html hit must not count.
        let top = s.stats_top_pages("wiki").await.unwrap();
        assert_eq!(top[0], ("/index.html".to_string(), 3));
        assert!(top.iter().all(|(p, _)| p != "/never.html"));
        assert_eq!(top.iter().map(|(_, c)| c).sum::<i64>(), 5);

        // Own count: all sources for (slug, email).
        assert_eq!(s.stats_own_count("wiki", "a@x.fr").await.unwrap(), 5);
        assert_eq!(s.stats_own_count("wiki", "b@x.fr").await.unwrap(), 2);
        assert_eq!(s.stats_own_count("wiki", "z@x.fr").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn prune_access_log_removes_only_old_rows() {
        let s = st("access-prune").await;
        s.log_access("a@x.fr", "wiki", "web", "/fresh.html")
            .await
            .unwrap();

        // One row 91 days old — beyond the 90-day retention window.
        sqlx::query(
            "INSERT INTO access_log (vault_slug, email, source, path, ts)  VALUES ('wiki', 'a@x.fr', 'web', '/old.html', datetime('now', '-91 days'))",
        )
        .execute(&s.pool)
        .await
        .unwrap();

        let removed = s.prune_access_log().await.unwrap();
        assert_eq!(removed, 1);

        let remaining: Vec<(String,)> = sqlx::query_as("SELECT path FROM access_log ORDER BY path")
            .fetch_all(&s.pool)
            .await
            .unwrap();
        assert_eq!(remaining, vec![("/fresh.html".to_string(),)]);

        // Idempotent: a second prune deletes nothing.
        assert_eq!(s.prune_access_log().await.unwrap(), 0);
    }
}
