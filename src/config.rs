//! Server configuration loading (`[server]`, `[session]`, `[storage]`, `[admin]`, ...);
//! admin credentials come from env vars or the `[admin]` section as fallback.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::email::SmtpConfig;

pub const DEFAULT_COOKIE_NAME: &str = "morinet_session";
pub const ENV_COOKIE_SECRET: &str = "MORINET_COOKIE_SECRET";
pub const ENV_ADMIN_EMAIL: &str = "MORINET_ADMIN_EMAIL";
pub const ENV_ADMIN_PASSWORD: &str = "MORINET_ADMIN_PASSWORD";
/// Phase 12 (D-01): comma-separated emails whose vault mutations are refused
/// on the public demo instance. Wins over the `[security]` file section.
pub const ENV_LOCKED_ACCOUNTS: &str = "MORINET_LOCKED_ACCOUNTS";

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
    /// Phase 5: outbound email. Absent/empty disables delivery — the app
    /// stays usable and the admin falls back to the manual flow.
    #[serde(default)]
    pub smtp: SmtpConfig,
    /// Bootstrap admin credentials (fallback when the env vars are absent).
    #[serde(default)]
    pub admin: AdminSection,
    /// Phase 12: locked-account guard (demo instance). Absent = nobody locked.
    #[serde(default)]
    pub security: SecuritySection,
}

/// `[security]` section: `locked_accounts = ["demo@x"]` — emails whose vault
/// mutations (load/delete/edit/share) are refused server-side. Env
/// `MORINET_LOCKED_ACCOUNTS` (comma-separated) takes precedence.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SecuritySection {
    #[serde(default)]
    pub locked_accounts: Vec<String>,
}

/// `[admin]` section: bootstrap admin credentials written by `morinet init`.
/// Env vars `MORINET_ADMIN_EMAIL` / `MORINET_ADMIN_PASSWORD` take precedence.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdminSection {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
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
    /// Phase 5: outbound email config (empty = delivery disabled).
    pub smtp: SmtpConfig,
    /// Root of the data directory. Phase 7: also the confinement root for the
    /// admin directory picker — an admin must not be able to browse `/etc`.
    pub data_dir: PathBuf,
    /// Directory containing the compiled mdbook (`book/`) to serve at `/vault/demo/`.
    pub vault_book_dir: Option<PathBuf>,
    /// Phase 12 (D-01): emails whose vault mutations are refused server-side
    /// (lowercase, trimmed). Empty = guard disabled.
    pub locked_accounts: Vec<String>,
}

/// Normalize a locked-accounts list: trim, lowercase, drop empties (T-12-03:
/// whitespace/case mistakes must never silently un-lock an account).
fn normalize_locked_list<'a>(entries: impl Iterator<Item = &'a str>) -> Vec<String> {
    entries
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
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
/// Admin credentials: `MORINET_ADMIN_EMAIL` / `MORINET_ADMIN_PASSWORD` env >
/// `[admin]` section in the config file (bootstrap only — the DB users table wins).
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
        .ok()
        .or_else(|| cfg.admin.email.clone().filter(|s| !s.is_empty()))
        .ok_or_else(|| anyhow::anyhow!("missing admin email: set {ENV_ADMIN_EMAIL}"))?;
    let admin_password = std::env::var(ENV_ADMIN_PASSWORD)
        .ok()
        .or_else(|| cfg.admin.password.clone().filter(|s| !s.is_empty()))
        .ok_or_else(|| anyhow::anyhow!("missing admin password: set {ENV_ADMIN_PASSWORD}"))?;
    // Phase 12 (D-01): env wins when set; never a hard error — empty default.
    let locked_accounts = std::env::var(ENV_LOCKED_ACCOUNTS)
        .ok()
        .map(|raw| normalize_locked_list(raw.split(',')))
        .or_else(|| {
            let from_file =
                normalize_locked_list(cfg.security.locked_accounts.iter().map(String::as_str));
            (!from_file.is_empty()).then_some(from_file)
        })
        .unwrap_or_default();

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
        smtp: cfg.smtp,
        data_dir: PathBuf::from(cfg.storage.data_dir),
        vault_book_dir,
        locked_accounts,
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
    fn parse_smtp_section_when_present() {
        let cfg: FileConfig = toml::from_str(
            r#"
[smtp]
host = "smtp.example.com"
port = 587
user = "bot@x.fr"
pass = "s3cret"
from = "bot@x.fr"
starttls = true
"#,
        )
        .unwrap();
        assert_eq!(cfg.smtp.host.as_deref(), Some("smtp.example.com"));
        assert_eq!(cfg.smtp.port, Some(587));
        assert_eq!(cfg.smtp.from.as_deref(), Some("bot@x.fr"));
        assert!(cfg.smtp.is_usable(), "full section is usable");
    }

    #[test]
    fn missing_smtp_section_is_empty_and_unusable() {
        let cfg: FileConfig = toml::from_str("[server]\n").unwrap();
        assert!(!cfg.smtp.is_usable(), "absent section disables delivery");
        assert!(cfg.smtp.host.is_none());
    }

    #[test]
    fn smtp_reaches_runtime_config() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<(&str, Option<String>)> = [
            (ENV_COOKIE_SECRET, std::env::var(ENV_COOKIE_SECRET).ok()),
            (ENV_ADMIN_EMAIL, std::env::var(ENV_ADMIN_EMAIL).ok()),
            (ENV_ADMIN_PASSWORD, std::env::var(ENV_ADMIN_PASSWORD).ok()),
        ]
        .into_iter()
        .collect();
        std::env::set_var(ENV_COOKIE_SECRET, "envsecret");
        std::env::set_var(ENV_ADMIN_EMAIL, "a@x.fr");
        std::env::set_var(ENV_ADMIN_PASSWORD, "pw");
        let cfg: FileConfig =
            toml::from_str("[smtp]\nhost = \"smtp.y\"\nfrom = \"bot@x.fr\"\n").unwrap();
        let rt = resolve(cfg, None).expect("resolve");
        assert!(rt.smtp.is_usable());
        assert_eq!(rt.smtp.host.as_deref(), Some("smtp.y"));
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

    #[test]
    fn file_config_without_admin_section_deserializes() {
        let cfg: FileConfig = toml::from_str("[server]\n").unwrap();
        assert!(cfg.admin.email.is_none());
        assert!(cfg.admin.password.is_none());
    }

    #[test]
    fn resolve_admin_from_file_section_when_env_absent() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<(&str, Option<String>)> = [
            (ENV_COOKIE_SECRET, std::env::var(ENV_COOKIE_SECRET).ok()),
            (ENV_ADMIN_EMAIL, std::env::var(ENV_ADMIN_EMAIL).ok()),
            (ENV_ADMIN_PASSWORD, std::env::var(ENV_ADMIN_PASSWORD).ok()),
        ]
        .into_iter()
        .collect();
        std::env::remove_var(ENV_COOKIE_SECRET);
        std::env::remove_var(ENV_ADMIN_EMAIL);
        std::env::remove_var(ENV_ADMIN_PASSWORD);
        let dir = std::env::temp_dir().join(format!("morinet-cfg-admin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_cfg(
            &dir,
            "[server]\n[session]\ncookie_secret = \"filesecret\"\n[admin]\nemail = \"file-admin@example.com\"\npassword = \"test-pass-9f3a\"\n",
        );
        let cfg = load_config(&path).unwrap();
        let rt = resolve(cfg, None).expect("resolve should fall back to [admin] section");
        assert_eq!(rt.admin_email, "file-admin@example.com");
        assert_eq!(rt.admin_password, "test-pass-9f3a");
        std::fs::remove_dir_all(&dir).ok();
        for (k, v) in saved {
            match v {
                Some(s) => std::env::set_var(k, s),
                None => std::env::remove_var(k),
            }
        }
    }

    #[test]
    fn resolve_env_admin_overrides_file_section() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<(&str, Option<String>)> = [
            (ENV_ADMIN_EMAIL, std::env::var(ENV_ADMIN_EMAIL).ok()),
            (ENV_ADMIN_PASSWORD, std::env::var(ENV_ADMIN_PASSWORD).ok()),
        ]
        .into_iter()
        .collect();
        std::env::set_var(ENV_ADMIN_EMAIL, "env-admin@example.com");
        std::env::set_var(ENV_ADMIN_PASSWORD, "env-pass");
        let cfg: FileConfig = toml::from_str(
            "[session]\ncookie_secret = \"filesecret\"\n[admin]\nemail = \"file-admin@example.com\"\npassword = \"test-pass-9f3a\"\n",
        )
        .unwrap();
        let rt = resolve(cfg, None).expect("resolve with env admin set");
        assert_eq!(rt.admin_email, "env-admin@example.com");
        assert_eq!(rt.admin_password, "env-pass");
        for (k, v) in saved {
            match v {
                Some(s) => std::env::set_var(k, s),
                None => std::env::remove_var(k),
            }
        }
    }

    /// Phase 12 (D-01): `MORINET_LOCKED_ACCOUNTS` wins over the `[security]`
    /// file section; entries are trimmed and lowercased.
    #[test]
    fn resolve_locked_accounts_env_overrides_file_section() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<(&str, Option<String>)> =
            [(ENV_LOCKED_ACCOUNTS, std::env::var(ENV_LOCKED_ACCOUNTS).ok())]
                .into_iter()
                .collect();
        std::env::set_var(ENV_LOCKED_ACCOUNTS, " Demo@Example.com ,other@x.fr");
        let cfg: FileConfig = toml::from_str(
            "[session]\ncookie_secret = \"filesecret\"\n[admin]\nemail = \"a@example.com\"\npassword = \"test-pass-9f3a\"\n[security]\nlocked_accounts = [\"file@example.com\"]\n",
        )
        .unwrap();
        let rt = resolve(cfg, None).expect("resolve with locked env set");
        assert_eq!(
            rt.locked_accounts,
            vec!["demo@example.com".to_string(), "other@x.fr".to_string()],
            "env wins over the file section, trimmed + lowercased"
        );
        for (k, v) in saved {
            match v {
                Some(s) => std::env::set_var(k, s),
                None => std::env::remove_var(k),
            }
        }
    }

    /// Phase 12 (D-01): no env, no `[security]` section → empty list, never an
    /// error (the guard is opt-in).
    #[test]
    fn resolve_locked_accounts_defaults_empty() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var(ENV_LOCKED_ACCOUNTS).ok();
        std::env::remove_var(ENV_LOCKED_ACCOUNTS);
        let cfg: FileConfig = toml::from_str(
            "[session]\ncookie_secret = \"filesecret\"\n[admin]\nemail = \"a@example.com\"\npassword = \"test-pass-9f3a\"\n",
        )
        .unwrap();
        let rt = resolve(cfg, None).expect("resolve without locked accounts");
        assert!(rt.locked_accounts.is_empty(), "default is empty");
        match saved {
            Some(s) => std::env::set_var(ENV_LOCKED_ACCOUNTS, s),
            None => std::env::remove_var(ENV_LOCKED_ACCOUNTS),
        }
    }

    /// Phase 12 (D-01): env absent → the `[security]` file section is used
    /// (entries lowercased so a typo cannot silently un-lock an account).
    #[test]
    fn resolve_locked_accounts_file_section_fallback() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var(ENV_LOCKED_ACCOUNTS).ok();
        std::env::remove_var(ENV_LOCKED_ACCOUNTS);
        let cfg: FileConfig = toml::from_str(
            "[session]\ncookie_secret = \"filesecret\"\n[admin]\nemail = \"a@example.com\"\npassword = \"test-pass-9f3a\"\n[security]\nlocked_accounts = [\" Demo@Example.com \"]\n",
        )
        .unwrap();
        let rt = resolve(cfg, None).expect("resolve with file section");
        assert_eq!(
            rt.locked_accounts,
            vec!["demo@example.com".to_string()],
            "file section used when env is absent"
        );
        match saved {
            Some(s) => std::env::set_var(ENV_LOCKED_ACCOUNTS, s),
            None => std::env::remove_var(ENV_LOCKED_ACCOUNTS),
        }
    }
}
