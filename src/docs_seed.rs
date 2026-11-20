//! Boot-seeded global documentation vault (REQ-VLT-013).
//!
//! Six user-facing English pages are embedded in the binary at compile time
//! and, on the very first boot of an instance, written to
//! `<data_dir>/docs/morinet-docs/` as a native mdBook source, built through
//! the normal mdbook pipeline and registered as the global vault
//! `morinet-docs` ("MoriNet Documentation"), granted read to every group.
//! A marker file (`<data_dir>/docs/.morinet-docs-seed`) makes the seed
//! strictly once-only: operator edits to the pages survive every restart.

use std::path::{Path, PathBuf};

use crate::sources::{LocalSource, VaultSource};
use crate::store::Store;

/// Slug of the embedded documentation vault.
pub const DOCS_SLUG: &str = "morinet-docs";
/// Display name of the embedded documentation vault.
pub const DOCS_NAME: &str = "MoriNet Documentation";
/// Marker file name; its presence means "already seeded, never rewrite".
const MARKER_NAME: &str = ".morinet-docs-seed";

const BOOK_TOML: &str = include_str!("../docs/morinet-docs/book.toml");
const SUMMARY_MD: &str = include_str!("../docs/morinet-docs/src/SUMMARY.md");
const QUICKSTART_MD: &str = include_str!("../docs/morinet-docs/src/quickstart.md");
const CONFIG_REFERENCE_MD: &str = include_str!("../docs/morinet-docs/src/config-reference.md");
const WEBDAV_MD: &str = include_str!("../docs/morinet-docs/src/webdav.md");
const MCP_MD: &str = include_str!("../docs/morinet-docs/src/mcp.md");
const SHARING_AND_ROLES_MD: &str = include_str!("../docs/morinet-docs/src/sharing-and-roles.md");
const FAQ_MD: &str = include_str!("../docs/morinet-docs/src/faq.md");

/// The eight embedded files: (relative path, content).
fn pages() -> [(&'static str, &'static str); 8] {
    [
        ("book.toml", BOOK_TOML),
        ("src/SUMMARY.md", SUMMARY_MD),
        ("src/quickstart.md", QUICKSTART_MD),
        ("src/config-reference.md", CONFIG_REFERENCE_MD),
        ("src/webdav.md", WEBDAV_MD),
        ("src/mcp.md", MCP_MD),
        ("src/sharing-and-roles.md", SHARING_AND_ROLES_MD),
        ("src/faq.md", FAQ_MD),
    ]
}

/// Write the embedded mdBook source tree under `dest` and return its path.
/// Existing files are NEVER overwritten — belt-and-braces alongside the seed
/// marker, so operator edits always win (T-10.1-15).
pub fn write_source_tree(dest: &Path) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dest.join("src"))?;
    for (rel, content) in pages() {
        let path = dest.join(rel);
        if !path.exists() {
            std::fs::write(&path, content)?;
        }
    }
    Ok(dest.to_path_buf())
}

/// Seed the global docs vault on first boot. Returns whether this boot
/// actually seeded (false = marker already present, nothing touched).
/// Any failure (mdbook missing, build error) is returned as `Err` — the
/// caller in `main.rs` only warns; boot never aborts (T-10.1-16).
pub async fn seed_docs_vault(data_dir: &Path, store: &Store) -> anyhow::Result<bool> {
    // Always sweep first (cheap, idempotent): groups created after the first
    // seed get read access on every boot.
    store.grant_vault_to_all_groups(DOCS_SLUG).await?;
    let docs_dir = data_dir.join("docs");
    let marker = docs_dir.join(MARKER_NAME);
    if marker.exists() {
        // Already seeded once — operator edits are never rewritten.
        return Ok(false);
    }
    let src_dir = write_source_tree(&docs_dir.join(DOCS_SLUG))?;
    let source = LocalSource {
        path: src_dir.clone(),
    };
    let book = source.build(&src_dir, DOCS_NAME)?;
    store.seed_vault(DOCS_SLUG, DOCS_NAME, &book).await?;
    // The sweep above ran before the vault row existed; grant now that it does.
    store.grant_vault_to_all_groups(DOCS_SLUG).await?;
    std::fs::create_dir_all(&docs_dir)?;
    std::fs::write(&marker, "1")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Temp dir derived from the test name (pitfalls #16/#23).
    fn temp_dir_for(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("morinet-docs-seed-{test}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_source_tree_lands_all_files() {
        let dir = temp_dir_for("write_source_tree_lands_all_files");
        let src = write_source_tree(&dir, false).unwrap();
        assert!(src.join("book.toml").is_file(), "book.toml missing");
        assert!(src.join("src/SUMMARY.md").is_file(), "SUMMARY.md missing");
        for page in [
            "quickstart.md",
            "config-reference.md",
            "webdav.md",
            "mcp.md",
            "sharing-and-roles.md",
            "faq.md",
        ] {
            assert!(
                src.join("src").join(page).is_file(),
                "embedded page missing: {page}"
            );
        }
        let quickstart = std::fs::read_to_string(src.join("src/quickstart.md")).unwrap();
        assert!(quickstart.contains("Quick start"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Coverage lock (D-01): every embedded content page carries OKF v0.2
    /// frontmatter per the B3Wiki house pattern (type/title/description/
    /// generated) and NEVER emits the trust fields verified/status/stale_after.
    #[test]
    fn docs_pages_carry_okf_frontmatter() {
        let all = pages();
        let content_pages: Vec<&(&str, &str)> = all
            .iter()
            .filter(|(rel, _)| rel.starts_with("src/") && *rel != "src/SUMMARY.md")
            .collect();
        assert_eq!(content_pages.len(), 6, "six content pages expected");
        for (rel, content) in content_pages {
            assert!(
                content.starts_with("---\n"),
                "{rel} must open with a frontmatter fence at byte 0"
            );
            let fm = content[4..]
                .split_once("\n---\n")
                .map(|(f, _)| f)
                .unwrap_or_else(|| panic!("{rel} has no closing frontmatter fence"));
            for key in ["type: ", "title: ", "description: ", "generated:"] {
                assert!(fm.contains(key), "{rel} frontmatter missing `{key}`");
            }
            assert!(
                fm.contains("by: process:morinet-docs"),
                "{rel} must stamp generated.by process:morinet-docs"
            );
            assert!(
                fm.contains("at: '2026-11-19T00:00:00Z'"),
                "{rel} must carry the fixed generated.at timestamp"
            );
            // Honesty rule (D-01): never emit trust claims.
            for trust in ["verified:", "status:", "stale_after:"] {
                assert!(
                    fm.lines().all(|l| !l.trim_start().starts_with(trust)),
                    "{rel} must not emit trust field `{trust}`"
                );
            }
        }
    }

    #[tokio::test]
    async fn seed_is_idempotent_and_preserves_edits() {
        let dir = temp_dir_for("seed_is_idempotent_and_preserves_edits");
        let store = Store::connect_memory().await.unwrap();
        store.ensure_builtin_groups().await.unwrap();

        // First pass: write the tree, then simulate the marker a successful
        // seed leaves behind (the build half needs mdbook — tested in the
        // #[ignore]d oracle below).
        let src = write_source_tree(&dir.join("docs").join(DOCS_SLUG), false).unwrap();
        std::fs::write(dir.join("docs").join(MARKER_NAME), SEED_MARKER_VALUE).unwrap();

        // Operator edits a page.
        let qs = src.join("src/quickstart.md");
        std::fs::write(&qs, "# Quick start\n\noperator edit survives restarts\n").unwrap();

        // Second boot: the marker short-circuits the seed entirely — no
        // rewrite, no build, no store writes.
        let seeded = seed_docs_vault(&dir, &store).await.unwrap();
        assert!(!seeded, "second boot must be a no-op");
        assert!(std::fs::read_to_string(&qs)
            .unwrap()
            .contains("operator edit survives restarts"));

        // write_source_tree itself never overwrites existing files either.
        write_source_tree(&src, false).unwrap();
        assert!(std::fs::read_to_string(&qs)
            .unwrap()
            .contains("operator edit survives restarts"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// D-03: a marker holding the OLD value ("1") must NOT short-circuit the
    /// seed — the forced re-seed overwrites operator edits with the embedded
    /// pages and deletes the stale compiled book/ (the Native build has a
    /// skip-when-compiled fast path, so a stale book would be served).
    #[test]
    fn stale_marker_triggers_reseed() {
        let dir = temp_dir_for("stale_marker_triggers_reseed");
        let src = write_source_tree(&dir, false).unwrap();
        let marker = dir.join(MARKER_NAME);
        std::fs::write(&marker, "1").unwrap();
        assert!(
            !marker_is_current(&marker),
            "a marker holding the old value must not be treated as current"
        );

        // Operator edit + stale compiled book from the old render.
        let qs = src.join("src/quickstart.md");
        std::fs::write(&qs, "operator edit\n").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        std::fs::write(src.join("book/index.html"), b"stale render").unwrap();

        reseed_source_tree(&dir).unwrap();
        assert!(
            std::fs::read_to_string(&qs)
                .unwrap()
                .contains("type: Guide"),
            "forced re-seed must restore the embedded page over the edit"
        );
        assert!(
            !src.join("book/index.html").exists(),
            "re-seed must delete the stale compiled book before rebuilding"
        );

        // A marker holding the current value short-circuits.
        std::fs::write(&marker, SEED_MARKER_VALUE).unwrap();
        assert!(marker_is_current(&marker));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// D-05: the seeded book.toml must carry the absolute current_exe path
    /// so mdBook can resolve the preprocessor regardless of the service PATH.
    #[test]
    fn seeded_book_toml_uses_absolute_preprocessor_command() {
        let dir = temp_dir_for("seeded_book_toml_absolute_command");
        let src = write_source_tree(&dir, false).unwrap();
        let toml = std::fs::read_to_string(src.join("book.toml")).unwrap();
        assert!(toml.contains("[preprocessor.okf-frontmatter]"));
        let exe = std::env::current_exe().unwrap();
        assert!(
            toml.contains(&format!(
                "command = \"{} mdbook-okf-preprocessor\"",
                exe.display()
            )),
            "seeded book.toml must rewrite the preprocessor command to the absolute current_exe"
        );
        assert!(
            !toml.contains("command = \"morinet mdbook-okf-preprocessor\""),
            "the relative literal must not survive seeding"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    #[ignore = "requires mdbook in PATH (like the corpus oracles)"]
    async fn seed_docs_vault_grants_all_groups() {
        let dir = temp_dir_for("seed_docs_vault_grants_all_groups");
        let store = Store::connect_memory().await.unwrap();
        store.ensure_builtin_groups().await.unwrap();
        store
            .create_user("member@x.fr", "pw", "reader")
            .await
            .unwrap();
        store.ensure_default_memberships().await.unwrap();

        let seeded = seed_docs_vault(&dir, &store).await.unwrap();
        assert!(seeded, "first boot must seed");

        // Global vault, visible with role read to a member of `free`.
        let vaults = store.accessible_vaults("member@x.fr").await.unwrap();
        let docs = vaults
            .iter()
            .find(|v| v.vault.slug == DOCS_SLUG)
            .expect("docs vault must be visible to any group member");
        assert_eq!(docs.role, "read");
        assert_eq!(docs.vault.scope, "global");
        assert_eq!(docs.vault.name, DOCS_NAME);
        assert_eq!(
            std::fs::read(dir.join("docs").join(MARKER_NAME)).unwrap(),
            SEED_MARKER_VALUE.as_bytes()
        );
        assert!(dir
            .join("docs")
            .join(DOCS_SLUG)
            .join("book/index.html")
            .is_file());

        // Second call is a no-op: marker present, exactly one vault row.
        assert!(!seed_docs_vault(&dir, &store).await.unwrap());
        let again = store.accessible_vaults("member@x.fr").await.unwrap();
        assert_eq!(
            again.iter().filter(|v| v.vault.slug == DOCS_SLUG).count(),
            1,
            "exactly one docs vault row after a second boot"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
