//! MoriNet — library surface (small in Phase 0, will grow with each phase).

pub mod config;
pub mod server;
pub mod session;
pub mod sources;
pub mod store;

/// Current package version (compile-time from Cargo.toml).
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Default config.toml template, returned as a `String` (TOML).
/// Phase 0: static. Phase 1+: dynamically generated from current defaults.
pub fn default_config() -> String {
    r#"# morinet — example config
# Copy to ./config.toml and edit values.

[server]
bind_addr = "127.0.0.1:8080"
base_url = "http://localhost:8080"

[storage]
data_dir = "./data"

[smtp]
host = "smtp.example.com"
port = 587
user = ""
pass = ""
from = "morinet@example.com"
starttls = true

[session]
# Generate via: openssl rand -hex 32
cookie_secret = "replace-me-with-32-bytes-of-random-hex"
cookie_name = "morinet_session"

[admin]
# Optional: bootstrap an admin on first boot if these are set in env
# MORINET_BOOTSTRAP_ADMIN_EMAIL / MORINET_BOOTSTRAP_ADMIN_PASSWORD

[vaults]
# [[vaults.entries]]
# slug = "demo"
# name = "Demo vault"
# kind = "local"
# path = "./data/vaults/demo/src"
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty(), "version() must return non-empty");
    }

    #[test]
    fn version_matches_cargo_pkg_version() {
        // Explicit guard: bump this alongside Cargo.toml on every release.
        assert_eq!(version(), "0.1.1");
    }

    #[test]
    fn default_config_contains_required_sections() {
        let cfg = default_config();
        assert!(cfg.contains("[server]"), "missing [server]");
        assert!(cfg.contains("[smtp]"), "missing [smtp]");
        assert!(cfg.contains("[storage]"), "missing [storage]");
        assert!(cfg.contains("[session]"), "missing [session]");
    }

    #[test]
    fn default_config_parses_as_valid_toml() {
        let cfg = default_config();
        let parsed: toml::Table = cfg.parse().expect("default_config() must be valid TOML");
        assert!(parsed.contains_key("server"));
        assert!(parsed.contains_key("smtp"));
        assert!(parsed.contains_key("storage"));
        assert!(parsed.contains_key("session"));
    }
}
