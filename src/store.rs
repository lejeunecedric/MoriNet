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
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub role: String,
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
                sqlx::query("INSERT INTO vaults (slug, name, book_path) VALUES (?, ?, ?)")
                    .bind(slug)
                    .bind(name)
                    .bind(&path_str)
                    .execute(&self.pool)
                    .await?;
            }
            Some((old_path,)) if old_path != path_str => {
                sqlx::query("UPDATE vaults SET name = ?, book_path = ? WHERE slug = ?")
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
        let row: Option<(i64, String, String, String)> =
            sqlx::query_as("SELECT id, email, password_hash, role FROM users WHERE email = ?")
                .bind(email)
                .fetch_optional(&self.pool)
                .await?;
        match row {
            Some((id, email, hash, role)) => {
                if bcrypt::verify(password, &hash)? {
                    Ok(Some(User { id, email, role }))
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
                let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
                    "SELECT v.id, v.slug, v.name, v.book_path \
                     FROM vaults v \
                     JOIN vault_acl a ON a.vault_id = v.id \
                     JOIN users u ON u.id = a.user_id \
                     WHERE u.email = ? \
                     ORDER BY v.slug",
                )
                .bind(email)
                .fetch_all(&self.pool)
                .await?;
                Ok(rows
                    .into_iter()
                    .map(|(id, slug, name, book_path)| Vault {
                        id,
                        slug,
                        name,
                        book_path,
                    })
                    .collect())
            }
        }
    }

    pub async fn list_all_vaults(&self) -> anyhow::Result<Vec<Vault>> {
        let rows: Vec<(i64, String, String, String)> =
            sqlx::query_as("SELECT id, slug, name, book_path FROM vaults ORDER BY slug")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|(id, slug, name, book_path)| Vault {
                id,
                slug,
                name,
                book_path,
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
        Ok(())
    }

    /// List all users (admin-ui table).
    pub async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        let rows: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT id, email, role FROM users ORDER BY email")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|(id, email, role)| User { id, email, role })
            .collect())
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

    fn unix_now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_secs()
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
}
