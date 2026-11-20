//! Boot-seeded global documentation vault (REQ-VLT-013).
//!
//! Six user-facing English pages are embedded in the binary at compile time
//! and, on the very first boot of an instance, written to
//! `<data_dir>/docs/morinet-docs/` as a native mdBook source, built through
//! the normal mdbook pipeline and registered as the global vault
//! `morinet-docs` ("MoriNet Documentation"), granted read to every group.
//! A marker file (`<data_dir>/docs/.morinet-docs-seed`) holds the seed
//! version: while its content equals `SEED_MARKER_VALUE` nothing is rewritten
//! (operator edits survive every restart). When a new binary ships a newer
//! marker value, every existing install re-seeds ONCE at next boot: the
//! embedded pages overwrite the vault (first-party content — replacing it is
//! the point, D-03) and the stale compiled `book/` is deleted so the new
//! content actually renders.

use std::path::{Path, PathBuf};

use crate::sources::{LocalSource, VaultSource};
use crate::store::Store;

/// Slug of the embedded documentation vault.
pub const DOCS_SLUG: &str = "morinet-docs";
/// Display name of the embedded documentation vault.
pub const DOCS_NAME: &str = "MoriNet Documentation";
/// Marker file name; the seed is skipped only when its content equals
/// `SEED_MARKER_VALUE`.
const MARKER_NAME: &str = ".morinet-docs-seed";
/// Current seed marker version. Bump when embedded docs content changes so
/// every existing install re-seeds once at next boot (D-03).
/// "2" = OKF frontmatter pages + render-time stripper shipped.
const SEED_MARKER_VALUE: &str = "2";

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

/// The literal preprocessor command shipped in the embedded `book.toml`.
/// Rewritten to an absolute path at seed time (D-05).
const PREPROCESSOR_COMMAND_LITERAL: &str = "command = \"morinet mdbook-okf-preprocessor\"";

/// Seed-time rewrite of the preprocessor command to the absolute
/// `current_exe` path, so mdBook resolves it regardless of the service PATH
/// (D-05). Note: mdBook splits the command string on whitespace, so a
/// `current_exe` path containing spaces fails the build LOUDLY with mdBook's
/// "command wasn't found" error — never silently. Falls back to the literal
/// when `current_exe` is unavailable.
fn seeded_book_toml(content: &str) -> String {
    seeded_book_toml_with(content, std::env::current_exe().ok().as_deref())
}

/// `seeded_book_toml` with the executable injected for testability (20-R3).
/// The path is escaped for a TOML basic string (backslashes and quotes) so
/// an install path containing `"` or `\` cannot break the file or inject
/// arbitrary keys into the seeded `book.toml`.
fn seeded_book_toml_with(content: &str, exe: Option<&std::path::Path>) -> String {
    let command = match exe {
        Some(exe) => {
            let escaped = exe
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            format!("command = \"{escaped} mdbook-okf-preprocessor\"")
        }
        None => PREPROCESSOR_COMMAND_LITERAL.to_string(),
    };
    content.replace(PREPROCESSOR_COMMAND_LITERAL, &command)
}

/// True when the marker exists and its trimmed content equals the current
/// seed version. A missing, unreadable or stale marker means "re-seed" (D-03).
fn marker_is_current(marker: &Path) -> bool {
    std::fs::read_to_string(marker)
        .map(|content| content.trim() == SEED_MARKER_VALUE)
        .unwrap_or(false)
}

/// Write the embedded mdBook source tree under `dest` and return its path.
/// With `force = false` existing files are NEVER overwritten — belt-and-braces
/// alongside the seed marker, so operator edits always win (T-10.1-15). With
/// `force = true` every file is overwritten: that is the marker-bump re-seed
/// path, where replacing first-party docs content is the point (D-03).
/// `book.toml` always gets the seed-time preprocessor command rewrite (D-05).
pub fn write_source_tree(dest: &Path, force: bool) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dest.join("src"))?;
    for (rel, content) in pages() {
        let path = dest.join(rel);
        if !force && path.exists() {
            continue;
        }
        let body = if rel == "book.toml" {
            std::borrow::Cow::Owned(seeded_book_toml(content))
        } else {
            std::borrow::Cow::Borrowed(content)
        };
        std::fs::write(&path, body.as_bytes())?;
    }
    Ok(dest.to_path_buf())
}

/// Re-seed half of a marker bump: force-write the embedded tree, then delete
/// the stale compiled `book/`. `sources.rs::build` has a Native
/// skip-when-compiled fast path (early return when `book/index.html` exists),
/// so without this delete a re-seed would serve the OLD rendered book and
/// never run the new preprocessor (D-05).
fn reseed_source_tree(dest: &Path) -> anyhow::Result<PathBuf> {
    let src = write_source_tree(dest, true)?;
    let _ = std::fs::remove_dir_all(src.join("book"));
    Ok(src)
}

/// Decide and run the tree write for a seed pass (marker already known not
/// to be current). The decision is made by TREE STATE, not marker presence
/// (20-R2): a tree already on disk with a non-current marker — stale OR
/// missing (crashed first boot: tree written, build failed before the
/// marker write) — forces the re-seed, so stale pages can never be baked
/// by the marker write that follows a successful build. Only a genuinely
/// absent tree takes the plain first-boot write.
fn seed_tree(src_dir: &Path) -> anyhow::Result<PathBuf> {
    if src_dir.join("book.toml").exists() {
        reseed_source_tree(src_dir)
    } else {
        write_source_tree(src_dir, false)
    }
}

/// Seed the global docs vault on first boot, or re-seed it once when the
/// marker holds a stale version (D-03). Returns whether this boot actually
/// seeded (false = marker already current, nothing touched).
/// Any failure (mdbook missing, build error) is returned as `Err` — the
/// caller in `main.rs` only warns; boot never aborts (T-10.1-16).
pub async fn seed_docs_vault(data_dir: &Path, store: &Store) -> anyhow::Result<bool> {
    // Always sweep first (cheap, idempotent): groups created after the first
    // seed get read access on every boot.
    store.grant_vault_to_all_groups(DOCS_SLUG).await?;
    let docs_dir = data_dir.join("docs");
    let marker = docs_dir.join(MARKER_NAME);
    if marker_is_current(&marker) {
        // Already seeded with the current content version — operator edits
        // are never rewritten.
        return Ok(false);
    }
    let src_dir = seed_tree(&docs_dir.join(DOCS_SLUG))?;
    let source = LocalSource {
        path: src_dir.clone(),
    };
    let book = source.build(&src_dir, DOCS_NAME)?;
    store.seed_vault(DOCS_SLUG, DOCS_NAME, &book).await?;
    // The sweep above ran before the vault row existed; grant now that it does.
    store.grant_vault_to_all_groups(DOCS_SLUG).await?;
    std::fs::create_dir_all(&docs_dir)?;
    std::fs::write(&marker, SEED_MARKER_VALUE)?;
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

    /// 20-R2 regression: marker MISSING + tree already present (crashed
    /// first boot: tree written, build failed before the marker write at
    /// line ~148) must take the forced re-seed path. The old marker-based
    /// branch treated this as first boot, ran the non-forcing write, built
    /// the stale tree, then stamped the marker "current" — baking stale
    /// v1 pages forever and skipping the D-03 re-seed.
    #[test]
    fn missing_marker_with_existing_tree_forces_reseed() {
        let dir = temp_dir_for("missing_marker_reseed");
        let src = write_source_tree(&dir, false).unwrap();
        let marker = dir.join(MARKER_NAME);
        assert!(
            !marker.exists(),
            "scenario: tree on disk, marker never written"
        );
        // Stale v1 leftovers from the failed first boot.
        let qs = src.join("src/quickstart.md");
        std::fs::write(&qs, "stale v1 page\n").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        std::fs::write(src.join("book/index.html"), b"stale render").unwrap();

        seed_tree(&src).unwrap();
        assert!(
            std::fs::read_to_string(&qs)
                .unwrap()
                .contains("type: Guide"),
            "missing marker + existing tree must force the re-seed"
        );
        assert!(
            !src.join("book/index.html").exists(),
            "forced re-seed must delete the stale compiled book"
        );
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

    /// 20-R3: a `current_exe` path containing `"` or `\` must be escaped for
    /// the TOML basic string — the seeded book.toml must stay parseable and
    /// must not allow arbitrary-key injection. Test added in the same commit
    /// as the one-line fix (LOW finding).
    #[test]
    fn seeded_book_toml_escapes_special_chars_in_exe_path() {
        let weird = std::path::Path::new("/tmp/we\"ird\\dir/morinet");
        let out = seeded_book_toml_with(BOOK_TOML, Some(weird));
        assert!(
            out.contains("command = \"/tmp/we\\\"ird\\\\dir/morinet mdbook-okf-preprocessor\""),
            "quotes/backslashes must be TOML-escaped"
        );
        out.parse::<toml::Value>()
            .expect("seeded book.toml must still parse as TOML");
        assert!(seeded_book_toml_with(BOOK_TOML, None).contains(PREPROCESSOR_COMMAND_LITERAL));
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
