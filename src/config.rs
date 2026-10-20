//! Server configuration loading (Phase 1: `[server]`, `[session]` sections;
//! admin credentials come from the environment until Phase 2 adds the DB).

use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const DEFAULT_COOKIE_NAME: &str = "morinet_session";
pub const ENV_COOKIE_SECRET: &str = "MORINET_COOKIE_SECRET";
pub const ENV_ADMIN_EMAIL: &str = "MORINET_ADMIN_EMAIL";
pub const ENV_ADMIN_PASSWORD: &str = "MORINET_ADMIN_PASSWORD";

#[derive(Debug, Clone, Deserialize)]
pub struct FileConfig {
    #[serde(default)]
    pub server: ServerSection,
    #[serde(default)]
    pub session: SessionSection,
    #[serde(default)]
    pub storage: StorageSection,
    #[serde(default)]
    pub vaults: VaultsSection,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StorageSection {
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct VaultsSection {
    #[serde(default)]
    pub entries: Vec<VaultEntry>,
    /// Background sync interval for `github` vaults, in seconds (default 300).
    #[serde(default)]
    pub sync_interval_secs: Option<u64>,
}

/// A vault registered in config (seeded into the DB at boot).
#[derive(Debug, Clone, Deserialize)]
pub struct VaultEntry {
    pub slug: String,
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    /// `local`: path to the compiled book dir. `github`: unused (see `url`).
    #[serde(default)]
    pub path: String,
    /// `github`: git URL to clone (https).
    #[serde(default)]
    pub url: String,
    /// `github`: branch (unused in v1 — default branch is tracked).
    #[serde(default)]
    pub branch: Option<String>,
    /// `github`: optional access token for private repos (injected into the URL).
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ServerSection {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SessionSection {
    pub cookie_secret: Option<String>,
    pub cookie_name: Option<String>,
}

fn default_bind_addr() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_data_dir() -> String {
    "./data".to_string()
}

fn default_kind() -> String {
    "local".to_string()
}

fn default_base_url() -> String {
    "http://localhost:8080".to_string()
}

/// Fully-resolved runtime configuration.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub bind_addr: String,
    pub base_url: String,
    pub cookie_secret: Vec<u8>,
    pub cookie_name: String,
    pub admin_email: String,
    pub admin_password: String,
    /// Directory containing the compiled mdbook (`book/`) to serve at `/vault/demo/`.
    pub vault_book_dir: Option<PathBuf>,
}

/// Load and parse the TOML config file.
pub fn load_config(path: &Path) -> anyhow::Result<FileConfig> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read config {}: {e}", path.display()))?;
    let cfg: FileConfig = toml::from_str(&raw)?;
    Ok(cfg)
}

/// Resolve the final runtime config: file values + env overrides.
///
/// Precedence for the cookie secret: `MORINET_COOKIE_SECRET` env > `[session].cookie_secret`.
/// Admin credentials come ONLY from env in Phase 1 (single hardcoded admin, staging-grade).
/// Phase 2 replaces this with the SQLite users table + bcrypt.
pub fn resolve(cfg: FileConfig, vault_book_dir: Option<PathBuf>) -> anyhow::Result<RuntimeConfig> {
    let cookie_secret = std::env::var(ENV_COOKIE_SECRET)
        .ok()
        .or(cfg.session.cookie_secret)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "missing session secret: set {ENV_COOKIE_SECRET} env or [session].cookie_secret in config"
            )
        })?;
    let admin_email = std::env::var(ENV_ADMIN_EMAIL)
        .map_err(|_| anyhow::anyhow!("missing admin email: set {ENV_ADMIN_EMAIL}"))?;
    let admin_password = std::env::var(ENV_ADMIN_PASSWORD)
        .map_err(|_| anyhow::anyhow!("missing admin password: set {ENV_ADMIN_PASSWORD}"))?;

    Ok(RuntimeConfig {
        bind_addr: cfg.server.bind_addr,
        base_url: cfg.server.base_url,
        cookie_secret: cookie_secret.into_bytes(),
        cookie_name: cfg
            .session
            .cookie_name
            .unwrap_or_else(|| DEFAULT_COOKIE_NAME.to_string()),
        admin_email,
        admin_password,
        vault_book_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize tests that mutate process-global env vars (cargo runs tests in
    /// parallel threads; env is shared, so these tests must not interleave).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn write_cfg(dir: &Path, content: &str) -> PathBuf {
        let p = dir.join("config.toml");
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn parse_minimal_config_with_defaults() {
        let dir = std::env::temp_dir().join(format!("morinet-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_cfg(&dir, "[server]\n");
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.server.bind_addr, "127.0.0.1:8080");
        assert_eq!(cfg.server.base_url, "http://localhost:8080");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_full_config_sections() {
        let dir = std::env::temp_dir().join(format!("morinet-cfg2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_cfg(
            &dir,
            "[server]\nbind_addr = \"0.0.0.0:9000\"\n[session]\ncookie_secret = \"filesecret\"\n",
        );
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.server.bind_addr, "0.0.0.0:9000");
        assert_eq!(cfg.session.cookie_secret.as_deref(), Some("filesecret"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_requires_cookie_secret() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Save + clear env to make the test deterministic.
        let saved = std::env::var(ENV_COOKIE_SECRET).ok();
        std::env::remove_var(ENV_COOKIE_SECRET);
        let cfg: FileConfig = toml::from_str("[server]\n").unwrap();
        let err = resolve(cfg, None).unwrap_err().to_string();
        assert!(err.contains("missing session secret"), "got: {err}");
        if let Some(v) = saved {
            std::env::set_var(ENV_COOKIE_SECRET, v);
        }
    }

    #[test]
    fn resolve_env_secret_overrides_file() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Save + set a deterministic env for the whole resolve (secret + admin creds).
        let saved: Vec<(&str, Option<String>)> = [
            (ENV_COOKIE_SECRET, std::env::var(ENV_COOKIE_SECRET).ok()),
            (ENV_ADMIN_EMAIL, std::env::var(ENV_ADMIN_EMAIL).ok()),
            (ENV_ADMIN_PASSWORD, std::env::var(ENV_ADMIN_PASSWORD).ok()),
        ]
        .into_iter()
        .collect();
        std::env::set_var(ENV_COOKIE_SECRET, "envsecret");
        std::env::set_var(ENV_ADMIN_EMAIL, "test-admin@example.com");
        std::env::set_var(ENV_ADMIN_PASSWORD, "test-pass");
        let cfg: FileConfig =
            toml::from_str("[server]\n[session]\ncookie_secret = \"filesecret\"\n").unwrap();
        let rt = resolve(cfg, None).expect("resolve with env secret set");
        assert_eq!(rt.cookie_secret, b"envsecret".to_vec());
        assert_eq!(rt.admin_email, "test-admin@example.com");
        for (k, v) in saved {
            match v {
                Some(s) => std::env::set_var(k, s),
                None => std::env::remove_var(k),
            }
        }
    }

    #[test]
    fn resolve_requires_admin_env() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved_secret = std::env::var(ENV_COOKIE_SECRET).ok();
        let saved_email = std::env::var(ENV_ADMIN_EMAIL).ok();
        let saved_pass = std::env::var(ENV_ADMIN_PASSWORD).ok();
        std::env::set_var(ENV_COOKIE_SECRET, "s3cret");
        std::env::remove_var(ENV_ADMIN_EMAIL);
        std::env::remove_var(ENV_ADMIN_PASSWORD);
        let cfg: FileConfig = toml::from_str("[server]\n").unwrap();
        let err = resolve(cfg, None).unwrap_err().to_string();
        assert!(err.contains("admin email"), "got: {err}");
        if let Some(v) = saved_secret {
            std::env::set_var(ENV_COOKIE_SECRET, v);
        }
        if let Some(v) = saved_email {
            std::env::set_var(ENV_ADMIN_EMAIL, v);
        }
        if let Some(v) = saved_pass {
            std::env::set_var(ENV_ADMIN_PASSWORD, v);
        }
    }
}
