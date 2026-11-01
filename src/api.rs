//! Shared read helpers for the REST API surface (Phase 16, D-04).
//!
//! The ACL resolution for the read surface lives HERE and only here: MCP
//! tools and REST handlers both call these free helpers, so the two
//! transports can never drift. An invisible vault yields `NotFound` —
//! never a 403-shaped, existence-leaking reason (D-02).

use std::path::{Path, PathBuf};

use crate::store::{AccessibleVault, Store};

/// Typed reason a read failed. Each transport maps these to its own error
/// shape: MCP to the legacy strings via [`ApiReadError::mcp_message`], REST
/// to RFC 9457 problems (wave 2/3). `NotFound` deliberately covers BOTH
/// unknown slug and no-access so nothing leaks vault existence (D-02).
#[derive(Debug, Clone, Copy)]
pub(crate) enum ApiReadError {
    /// Vault invisible to this user (unknown slug OR no access — D-02).
    NotFound,
    /// Traversal / absolute path refused by `safe_join`.
    InvalidPath,
    /// Requested sub-folder does not exist inside the vault.
    NoSuchFolder,
    /// Requested note file does not exist inside the vault.
    NoSuchNote,
    /// File exists but could not be read.
    Unreadable,
}

impl ApiReadError {
    /// The EXACT legacy MCP error strings (test-asserted verbatim at
    /// mcp.rs `mcp_hidden_share_invisible` / `mcp_revoked_share_denied`).
    pub(crate) fn mcp_message(&self) -> &'static str {
        match self {
            ApiReadError::NotFound => "vault not found or access denied",
            ApiReadError::InvalidPath => "invalid path",
            ApiReadError::NoSuchFolder => "no such folder",
            ApiReadError::NoSuchNote => "no such note",
            ApiReadError::Unreadable => "could not read the note",
        }
    }
}

/// Every vault the user can access (personal + shares). Store errors degrade
/// to an empty list, same as the legacy MCP bodies (`unwrap_or_default`).
pub(crate) async fn accessible_vault_entries(store: &Store, email: &str) -> Vec<AccessibleVault> {
    store.accessible_vaults(email).await.unwrap_or_default()
}

/// Vault JSON with EXACTLY the MCP field names — slug/name/scope/role, no
/// envelope (D-08). REST serves the same object (D-08 field parity).
pub(crate) fn vault_json(a: &AccessibleVault) -> serde_json::Value {
    serde_json::json!({
        "slug": a.vault.slug,
        "name": a.vault.name,
        "scope": a.vault.scope,
        "role": a.role,
    })
}

/// The source directory of an accessible vault (same resolution as
/// WebDAV: the markdown source, never the generated tree — D-06). One
/// generic `None` for unknown slug and no-access — no existence leak (D-02).
pub(crate) async fn source_dir(store: &Store, email: &str, slug: &str) -> Option<PathBuf> {
    let entry = accessible_vault_entries(store, email)
        .await
        .into_iter()
        .find(|a| a.vault.slug == slug)?;
    crate::webdav::source_dir_of(Path::new(&entry.vault.book_path))
}

/// Sorted `.md` paths under `sub` (or the vault root) of an accessible
/// vault. Sorted is load-bearing: the wave-3 cursor pages over exactly
/// this order (D-09).
pub(crate) async fn list_notes(
    store: &Store,
    email: &str,
    slug: &str,
    sub: Option<&str>,
) -> Result<Vec<String>, ApiReadError> {
    let src = source_dir(store, email, slug)
        .await
        .ok_or(ApiReadError::NotFound)?;
    let base = match sub {
        Some(s) => crate::mcp::safe_join(&src, s).ok_or(ApiReadError::InvalidPath)?,
        None => src,
    };
    if !base.is_dir() {
        return Err(ApiReadError::NoSuchFolder);
    }
    Ok(crate::mcp::list_md_files(&base))
}

/// Raw bytes of one markdown note (REST needs bytes for `webdav::etag_of`;
/// the MCP wrapper converts to String itself).
pub(crate) async fn read_note(
    store: &Store,
    email: &str,
    slug: &str,
    path: &str,
) -> Result<Vec<u8>, ApiReadError> {
    let src = source_dir(store, email, slug)
        .await
        .ok_or(ApiReadError::NotFound)?;
    let full = crate::mcp::safe_join(&src, path).ok_or(ApiReadError::InvalidPath)?;
    if !full.is_file() {
        return Err(ApiReadError::NoSuchNote);
    }
    std::fs::read(&full).map_err(|_| ApiReadError::Unreadable)
}

/// Case-insensitive substring hits across accessible vaults (or one slug),
/// capped at 50 (D-10, mirrors the MCP `search_md` cap).
pub(crate) async fn search_notes(
    store: &Store,
    email: &str,
    query: &str,
    slug: Option<&str>,
) -> Result<Vec<crate::mcp::Hit>, ApiReadError> {
    let vaults: Vec<(String, PathBuf)> = match slug {
        Some(s) => match source_dir(store, email, s).await {
            Some(dir) => vec![(s.to_string(), dir)],
            None => return Err(ApiReadError::NotFound),
        },
        None => accessible_vault_entries(store, email)
            .await
            .iter()
            .filter_map(|a| {
                crate::webdav::source_dir_of(Path::new(&a.vault.book_path))
                    .map(|dir| (a.vault.slug.clone(), dir))
            })
            .collect(),
    };
    Ok(crate::mcp::search_md(&vaults, query, 50))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::path::PathBuf;

    async fn store() -> Store {
        Store::connect_memory().await.expect("memory store")
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("morinet-api-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Admin-owned vault seeded with `note.md`; returns (store, src_dir).
    async fn seed_vault(tag: &str, slug: &str) -> (Store, PathBuf) {
        let s = store().await;
        s.ensure_admin("admin@api.test", "pw").await.unwrap();
        let src = tmpdir(tag);
        std::fs::write(src.join("note.md"), "# Note\ncontent").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault(slug, "API vault", &src.join("book"))
            .await
            .unwrap();
        (s, src)
    }

    #[tokio::test]
    async fn helper_source_dir_invisible_vault_is_none() {
        let (s, _src) = seed_vault("srcdir", "sv1").await;
        s.create_user("stranger@api.test", "pw", "reader")
            .await
            .unwrap();
        // Visible to the admin, invisible to a stranger: None, never an error.
        assert!(source_dir(&s, "admin@api.test", "sv1").await.is_some());
        assert!(source_dir(&s, "stranger@api.test", "sv1").await.is_none());
        assert!(source_dir(&s, "admin@api.test", "nope").await.is_none());
    }

    #[tokio::test]
    async fn helper_list_notes_not_found_vs_invalid_path() {
        let (s, _src) = seed_vault("listnotes", "ln1").await;
        // Unknown slug → NotFound (same shape as no-access: D-02).
        assert!(matches!(
            list_notes(&s, "admin@api.test", "unknown", None).await,
            Err(ApiReadError::NotFound)
        ));
        // Traversal sub → InvalidPath, a DIFFERENT reason.
        assert!(matches!(
            list_notes(&s, "admin@api.test", "ln1", Some("../x")).await,
            Err(ApiReadError::InvalidPath)
        ));
        // Happy path: sorted .md list of the source root.
        let files = list_notes(&s, "admin@api.test", "ln1", None).await.unwrap();
        assert_eq!(files, vec!["note.md".to_string()]);
        // Missing sub-folder → NoSuchFolder.
        assert!(matches!(
            list_notes(&s, "admin@api.test", "ln1", Some("ghost")).await,
            Err(ApiReadError::NoSuchFolder)
        ));
    }

    #[tokio::test]
    async fn helper_read_note_maps_reasons() {
        let (s, _src) = seed_vault("readnote", "rn1").await;
        // Missing file → NoSuchNote.
        assert!(matches!(
            read_note(&s, "admin@api.test", "rn1", "ghost.md").await,
            Err(ApiReadError::NoSuchNote)
        ));
        // Traversal → InvalidPath.
        assert!(matches!(
            read_note(&s, "admin@api.test", "rn1", "../secret.md").await,
            Err(ApiReadError::InvalidPath)
        ));
        // Invisible vault → NotFound.
        assert!(matches!(
            read_note(&s, "admin@api.test", "nope", "note.md").await,
            Err(ApiReadError::NotFound)
        ));
        // Happy path: raw bytes of the source note.
        let bytes = read_note(&s, "admin@api.test", "rn1", "note.md")
            .await
            .unwrap();
        assert_eq!(bytes, b"# Note\ncontent");
    }

    #[tokio::test]
    async fn helper_search_scoped_to_accessible() {
        let s = store().await;
        s.create_user("owner@api.test", "pw", "reader")
            .await
            .unwrap();
        s.create_user("sharee@api.test", "pw", "reader")
            .await
            .unwrap();
        s.create_user("stranger@api.test", "pw", "reader")
            .await
            .unwrap();
        let src = tmpdir("search");
        std::fs::write(src.join("note.md"), "# Shared\nsecret needle content").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault("ss1", "Shared vault", &src.join("book"))
            .await
            .unwrap();
        s.share_vault("ss1", "sharee@api.test", "reader")
            .await
            .unwrap();

        // Sharee finds the hit in the shared vault.
        let hits = search_notes(&s, "sharee@api.test", "secret needle", None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slug, "ss1");
        // Stranger finds nothing.
        let none = search_notes(&s, "stranger@api.test", "secret needle", None)
            .await
            .unwrap();
        assert!(none.is_empty());
        // Scoped search on an invisible slug → NotFound.
        assert!(matches!(
            search_notes(&s, "stranger@api.test", "secret needle", Some("ss1")).await,
            Err(ApiReadError::NotFound)
        ));
    }

    #[tokio::test]
    async fn helper_vault_json_field_names() {
        let (s, _src) = seed_vault("vaultjson", "vj1").await;
        let entries = accessible_vault_entries(&s, "admin@api.test").await;
        let entry = entries.iter().find(|a| a.vault.slug == "vj1").unwrap();
        let j = vault_json(entry);
        // EXACTLY the MCP field set (D-08): slug/name/scope/role, no extras.
        let obj = j.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, vec!["name", "role", "scope", "slug"]);
        assert_eq!(obj["slug"].as_str().unwrap(), "vj1");
        assert_eq!(obj["name"].as_str().unwrap(), "API vault");
    }
}
