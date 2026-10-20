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
