//! Phase 18 (OKF) — core module: conformance scan, trust tier, note metadata,
//! bundle export.
//!
//! Pure functions over a directory tree and parsed frontmatter — no store, no
//! async (D-04/D-10). The conformance rules follow OKF
//! SPEC v0.2 §11 as locked in D-03; trust tiers follow §5.3 (D-07). The only
//! dependency used is the single permitted archive crate `zip` (D-06).

use serde::{Deserialize, Serialize};

use crate::obsidian::frontmatter::{self, Frontmatter};

/// Frontmatter keys whose presence on any concept implies a v0.2 bundle
/// (D-03 version guess). Presence is a flag only — nested structures stay
/// opaque per D-04.
const V02_KEYS: [&str; 5] = ["generated", "verified", "sources", "status", "stale_after"];

/// Result of an OKF conformance scan over one vault source directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfStats {
    /// `"0.2"` / `"0.1"` / `None` (= not an OKF bundle).
    pub version: Option<String>,
    /// Non-reserved `.md` files with a non-empty `type:`.
    pub concepts: usize,
    /// Non-reserved `.md` files seen (unreadable ones count here only).
    pub total: usize,
    pub has_index: bool,
    pub has_log: bool,
    /// Epoch seconds at scan time.
    pub scanned_at: u64,
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_secs()
}

/// Scan `dir` (a vault source tree) for OKF conformance (D-03).
///
/// Pure and best-effort: hidden dirs (`.morinet-build/`, `.git/`, …) are
/// skipped via the proven `list_md_files` walk; unreadable or non-UTF-8
/// files count toward `total` but never `concepts`; a scan never panics on
/// hostile input. `index.md`/`log.md` at the root are reserved: presence is
/// noted, they are excluded from the concepts/total denominators.
pub fn scan_conformance(dir: &std::path::Path) -> anyhow::Result<OkfStats> {
    let mut concepts = 0usize;
    let mut total = 0usize;
    let mut has_index = false;
    let mut has_log = false;
    let mut v02 = false;
    for rel in crate::mcp::list_md_files(dir) {
        if rel == "index.md" {
            has_index = true;
            continue;
        }
        if rel == "log.md" {
            has_log = true;
            continue;
        }
        total += 1;
        let Ok(md) = std::fs::read_to_string(dir.join(&rel)) else {
            continue; // unreadable / non-UTF-8: total only, never a concept
        };
        let (fm, _) = frontmatter::split(&md);
        if fm.page_type().is_some_and(|t| !t.trim().is_empty()) {
            concepts += 1;
            if V02_KEYS.iter().any(|k| fm.has(k)) {
                v02 = true;
            }
        }
    }
    let version = if total > 0 && concepts * 100 >= total * 80 {
        Some(if v02 { "0.2" } else { "0.1" }.to_string())
    } else {
        None
    };
    Ok(OkfStats {
        version,
        concepts,
        total,
        has_index,
        has_log,
        scanned_at: unix_now(),
    })
}

/// OKF §5.3 trust tier (D-07): no `verified` key → `unverified`; `verified`
/// present with no `human:` actor → `machine-confirmed`; any `human:` actor
/// → `human-reviewed`. Block-list-of-maps items parse flat (e.g.
/// `"by: human:alice"`), so a substring check on each candidate suffices.
/// Display/metadata only — never an authorization input (T-18-02).
pub fn trust_tier(fm: &Frontmatter) -> &'static str {
    if !fm.has("verified") {
        return "unverified";
    }
    let has_human = fm.list("verified").iter().any(|c| c.contains("human:"))
        || fm.scalar("verified").is_some_and(|s| s.contains("human:"));
    if has_human {
        "human-reviewed"
    } else {
        "machine-confirmed"
    }
}

/// D-07 frontmatter metadata block for API/MCP surfaces: nulls for absent
/// fields, trust tier derived (never stored).
pub fn note_meta(fm: &Frontmatter) -> serde_json::Value {
    serde_json::json!({
        "type": fm.page_type(),
        "tags": fm.list("tags"),
        "status": fm.scalar("status"),
        "trust_tier": trust_tier(fm),
    })
}

/// Shared "no frontmatter ⇒ null" rule (D-07): `None` for non-UTF-8 bytes,
/// absent blocks, and empty blocks alike.
pub fn frontmatter_of(bytes: &[u8]) -> Option<Frontmatter> {
    let s = std::str::from_utf8(bytes).ok()?;
    let (fm, _) = frontmatter::split(s);
    if fm.is_empty() {
        None
    } else {
        Some(fm)
    }
}

/// Entry-count bound for [`build_bundle_zip`] (T-18-10). The zip is built
/// in-memory per D-06's simplicity call, so the walk must stop before a
/// hostile tree can exhaust memory. The existing quota convention is
/// vault-COUNT not bytes, so an explicit constant is the honest bound.
pub const MAX_EXPORT_ENTRIES: usize = 20_000;
/// Total-source-bytes bound for [`build_bundle_zip`] (T-18-10): 256 MiB.
pub const MAX_EXPORT_BYTES: u64 = 268_435_456;

/// Pure caps guard called DURING the walk, before any file is buffered
/// (T-18-10): rejects once the running totals reach the caps.
pub(crate) fn check_export_caps(entries: usize, bytes: u64) -> anyhow::Result<()> {
    if entries >= MAX_EXPORT_ENTRIES || bytes >= MAX_EXPORT_BYTES {
        anyhow::bail!(
            "bundle too large: exceeds {MAX_EXPORT_BYTES} bytes or {MAX_EXPORT_ENTRIES} entries"
        );
    }
    Ok(())
}

/// In-memory OKF bundle of a vault source tree (D-06, OKF-03): every file
/// below `src` byte-identical, dot-directories (`.morinet-build/`, `.git/`,
/// …) excluded, entries `/`-separated relative paths. When the walk finds no
/// root `index.md`, a virtual entry is synthesized in-memory ONLY (never
/// written to disk) carrying `okf_version: "0.2"` frontmatter.
///
/// Guards: each file is canonicalized and must stay under `src` — a symlink
/// planted inside the vault that resolves outside is refused (T-18-08);
/// sizes/counts are bounded by [`check_export_caps`] (T-18-10).
pub fn build_bundle_zip(src: &std::path::Path) -> anyhow::Result<Vec<u8>> {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let root = src.canonicalize()?;
    let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut entries = 0usize;
    let mut bytes = 0u64;
    let mut has_index = false;
    // Explicit stack, same idiom as the frontmatter oracle walk.
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // Dot-files and dot-dirs never ship (REV-18 M-02: a committed
            // `.env` must not reach every reader).
            if name.starts_with('.') {
                continue;
            }
            // lstat semantics: never follow symlinks at all (REV-18 M-04 +
            // LOW dup-entries) — an escaping symlink is a traversal attempt
            // (T-18-08) and an internal one would duplicate its target entry.
            // One bad link must not break export for everyone: skip + log.
            let ftype = entry.file_type()?;
            if ftype.is_symlink() {
                eprintln!("okf-export: skipping symlink: {}", path.display());
                continue;
            }
            if ftype.is_dir() {
                stack.push(path);
                continue;
            }
            if !ftype.is_file() {
                continue; // fifos, sockets, devices: never bundle.
            }
            // Defense in depth (T-18-08): canonicalize and stay under root.
            let canon = path.canonicalize()?;
            if !canon.starts_with(&root) {
                eprintln!(
                    "okf-export: skipping path outside source: {}",
                    path.display()
                );
                continue;
            }
            let rel = canon
                .strip_prefix(&root)?
                .components()
                .collect::<Vec<_>>()
                .iter()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if rel == "index.md" {
                has_index = true;
            }
            // Caps checked on the metadata BEFORE buffering the bytes.
            let len = std::fs::metadata(&canon)?.len();
            check_export_caps(entries + 1, bytes + len)?;
            let data = std::fs::read(&canon)?;
            entries += 1;
            bytes += len;
            zip.start_file(rel, options)?;
            zip.write_all(&data)?;
        }
    }
    if !has_index {
        zip.start_file("index.md", options)?;
        zip.write_all(b"---\nokf_version: \"0.2\"\n---\n\n# Bundle index\n")?;
    }
    Ok(zip.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obsidian::frontmatter;
    use std::path::{Path, PathBuf};

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("morinet-okf-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn page(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, content).unwrap();
    }

    fn typed(title: &str) -> String {
        format!("---\ntype: concept\ntitle: {title}\n---\n\n{title} body.\n")
    }

    #[test]
    fn scan_v02_bundle() {
        let d = tmpdir("v02");
        page(&d, "a.md", &typed("A"));
        page(
            &d,
            "b.md",
            "---\ntype: concept\nstatus: draft\nverified:\n  - by: human:alice\n    at: 2026-11-01\n---\n\nB body.\n",
        );
        page(&d, "c.md", &typed("C"));
        page(&d, "d.md", &typed("D"));
        page(&d, "index.md", "# Index\n");
        page(&d, "log.md", "# Log\n");
        let s = scan_conformance(&d).unwrap();
        assert_eq!(s.version.as_deref(), Some("0.2"));
        assert_eq!(s.concepts, 4);
        assert_eq!(s.total, 4);
        assert!(s.has_index);
        assert!(s.has_log);
        assert!(s.scanned_at > 1_700_000_000);
    }

    #[test]
    fn scan_v01_bundle() {
        let d = tmpdir("v01");
        page(&d, "a.md", &typed("A"));
        page(&d, "b.md", &typed("B"));
        page(&d, "sub/c.md", &typed("C"));
        let s = scan_conformance(&d).unwrap();
        assert_eq!(s.version.as_deref(), Some("0.1"));
        assert_eq!(s.concepts, 3);
        assert_eq!(s.total, 3);
        assert!(!s.has_index);
        assert!(!s.has_log);
    }

    #[test]
    fn scan_below_80_percent_is_not_okf() {
        let d = tmpdir("below80");
        page(&d, "a.md", &typed("A"));
        page(&d, "b.md", &typed("B"));
        page(&d, "c.md", &typed("C"));
        page(&d, "d.md", "# No frontmatter at all\n");
        let s = scan_conformance(&d).unwrap();
        assert_eq!(s.version, None);
        assert_eq!(s.concepts, 3);
        assert_eq!(s.total, 4);
    }

    #[test]
    fn scan_exactly_80_percent_is_okf() {
        let d = tmpdir("exact80");
        for i in 0..4 {
            page(&d, &format!("t{i}.md"), &typed("T"));
        }
        page(&d, "u.md", "# untyped\n");
        let s = scan_conformance(&d).unwrap();
        assert_eq!(s.version.as_deref(), Some("0.1"));
        assert_eq!(s.concepts, 4);
        assert_eq!(s.total, 5);
    }

    #[test]
    fn scan_empty_dir_is_not_okf() {
        let d = tmpdir("empty");
        let s = scan_conformance(&d).unwrap();
        assert_eq!(s.version, None);
        assert_eq!(s.total, 0);
        assert_eq!(s.concepts, 0);
    }

    #[test]
    fn scan_skips_hidden_dirs_and_counts_unreadable() {
        let d = tmpdir("hidden-unreadable");
        for i in 0..4 {
            page(&d, &format!("t{i}.md"), &typed("T"));
        }
        // Invalid UTF-8: counts toward total only, never panics.
        std::fs::write(d.join("bad.md"), [0xff_u8, 0xfe, 0x00, 0x80]).unwrap();
        // Generated book tree must be ignored entirely.
        page(&d, ".morinet-build/book.md", &typed("ghost"));
        let s = scan_conformance(&d).unwrap();
        assert_eq!(s.total, 5);
        assert_eq!(s.concepts, 4);
        assert_eq!(s.version.as_deref(), Some("0.1"));
    }

    #[test]
    fn scan_reserved_files_excluded_from_denominator() {
        let d = tmpdir("reserved");
        for i in 0..4 {
            page(&d, &format!("t{i}.md"), &typed("T"));
        }
        // index.md without type must not drag the ratio below 80%.
        page(&d, "index.md", "# Index\n");
        page(&d, "log.md", "# Log\n");
        let s = scan_conformance(&d).unwrap();
        assert_eq!(s.total, 4);
        assert_eq!(s.concepts, 4);
        assert_eq!(s.version.as_deref(), Some("0.1"));
        assert!(s.has_index);
        assert!(s.has_log);
    }

    #[test]
    fn trust_tier_absent_verified_is_unverified() {
        let (fm, _) = frontmatter::split("---\ntype: concept\n---\n");
        assert_eq!(trust_tier(&fm), "unverified");
    }

    #[test]
    fn trust_tier_machine_actor_is_machine_confirmed() {
        let (fm, _) = frontmatter::split(
            "---\ntype: concept\nverified:\n  - by: llm-wiki:gpt\n    at: 2026-11-01\n---\n",
        );
        assert_eq!(trust_tier(&fm), "machine-confirmed");
    }

    #[test]
    fn trust_tier_human_actor_is_human_reviewed() {
        let (fm, _) = frontmatter::split(
            "---\ntype: concept\nverified:\n  - by: human:alice\n    at: 2026-11-01\n---\n",
        );
        assert_eq!(trust_tier(&fm), "human-reviewed");
    }

    #[test]
    fn trust_tier_scalar_verified_human() {
        let (fm, _) = frontmatter::split("---\ntype: concept\nverified: human:bob\n---\n");
        assert_eq!(trust_tier(&fm), "human-reviewed");
        let (fm2, _) = frontmatter::split("---\ntype: concept\nverified: llm:gpt\n---\n");
        assert_eq!(trust_tier(&fm2), "machine-confirmed");
    }

    #[test]
    fn note_meta_shape_nulls_for_missing_fields() {
        let (fm, _) = frontmatter::split("---\ntype: concept\ntags: [a, b]\n---\n");
        let m = note_meta(&fm);
        assert_eq!(m["type"], serde_json::json!("concept"));
        assert_eq!(m["tags"], serde_json::json!(["a", "b"]));
        assert_eq!(m["status"], serde_json::Value::Null);
        assert_eq!(m["trust_tier"], serde_json::json!("unverified"));

        let (fm2, _) = frontmatter::split("---\ntitle: bare\n---\n");
        let m2 = note_meta(&fm2);
        assert_eq!(m2["type"], serde_json::Value::Null);
        assert_eq!(m2["tags"], serde_json::json!([]));
        assert_eq!(m2["status"], serde_json::Value::Null);
        assert_eq!(m2["trust_tier"], serde_json::json!("unverified"));
    }

    #[test]
    fn frontmatter_of_none_without_frontmatter() {
        assert!(frontmatter_of(b"# just a page\n").is_none());
        assert!(frontmatter_of(&[0xff_u8, 0xfe]).is_none());
        let fm = frontmatter_of(b"---\ntype: entity\n---\nbody\n").unwrap();
        assert_eq!(fm.page_type(), Some("entity"));
    }

    // ---------- 18-04: OKF bundle zip (D-06, OKF-03) ----------

    fn zip_names(bytes: &[u8]) -> Vec<String> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        (0..z.len())
            .map(|i| z.by_index(i).unwrap().name().unwrap().to_string())
            .collect()
    }

    fn zip_entry(bytes: &[u8], name: &str) -> Vec<u8> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        let mut f = z.by_name(name).unwrap();
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut out).unwrap();
        out
    }

    #[test]
    fn bundle_zip_entries_byte_identical() {
        let d = tmpdir("zip-identical");
        page(&d, "a.md", "# A\nalpha\n");
        page(&d, "sub/b.md", "# B\nbeta\n");
        let bytes = build_bundle_zip(&d).unwrap();
        let names = zip_names(&bytes);
        assert!(names.contains(&"a.md".to_string()), "{names:?}");
        assert!(names.contains(&"sub/b.md".to_string()), "{names:?}");
        assert_eq!(
            zip_entry(&bytes, "a.md"),
            std::fs::read(d.join("a.md")).unwrap()
        );
        assert_eq!(
            zip_entry(&bytes, "sub/b.md"),
            std::fs::read(d.join("sub/b.md")).unwrap()
        );
    }

    #[test]
    fn bundle_zip_skips_dot_dirs() {
        let d = tmpdir("zip-dotdirs");
        page(&d, "a.md", "# A\n");
        page(&d, ".morinet-build/x.md", "generated ghost\n");
        page(&d, ".git/y.md", "git ghost\n");
        let names = zip_names(&build_bundle_zip(&d).unwrap());
        assert!(names.contains(&"a.md".to_string()), "{names:?}");
        assert!(
            !names.iter().any(|n| n.contains(".morinet-build")),
            "{names:?}"
        );
        assert!(!names.iter().any(|n| n.contains(".git")), "{names:?}");
    }

    #[test]
    fn bundle_zip_synthesizes_index_when_absent() {
        let d = tmpdir("zip-virtual-index");
        page(&d, "a.md", "# A\n");
        let bytes = build_bundle_zip(&d).unwrap();
        let idx = zip_entry(&bytes, "index.md");
        assert!(String::from_utf8(idx)
            .unwrap()
            .contains("okf_version: \"0.2\""));
        // D-06: the virtual entry is in-memory only — never written to disk.
        assert!(!d.join("index.md").exists());
    }

    #[test]
    fn bundle_zip_keeps_real_index() {
        let d = tmpdir("zip-real-index");
        page(&d, "index.md", "# Real index\n");
        page(&d, "a.md", "# A\n");
        let bytes = build_bundle_zip(&d).unwrap();
        let names = zip_names(&bytes);
        assert_eq!(names.iter().filter(|n| *n == "index.md").count(), 1);
        assert_eq!(zip_entry(&bytes, "index.md"), b"# Real index\n");
    }

    #[test]
    fn bundle_zip_skips_symlink_escape() {
        // REV-18 M-04: an escaping symlink must NOT break the whole bundle —
        // it is skipped (and never followed), the rest still exports.
        let outside = tmpdir("zip-outside");
        std::fs::write(outside.join("secret.md"), "outside secret\n").unwrap();
        let d = tmpdir("zip-symlink");
        page(&d, "a.md", "# A\n");
        std::os::unix::fs::symlink(outside.join("secret.md"), d.join("link.md")).unwrap();
        let bytes = build_bundle_zip(&d).expect("bundle builds despite escaping symlink");
        let z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let names: Vec<String> = z
            .file_names()
            .filter_map(|n| n.ok().map(|s| s.into_owned()))
            .collect();
        assert!(names.contains(&"a.md".to_string()));
        assert!(!names.iter().any(|n| n.contains("secret")));
    }

    #[test]
    fn bundle_zip_skips_dotfiles_and_dotdirs() {
        // REV-18 M-02: a committed `.env` must never reach readers.
        let d = tmpdir("zip-dotfiles");
        page(&d, "a.md", "# A\n");
        std::fs::write(d.join(".env"), "TOKEN=s3cr3t\n").unwrap();
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::write(d.join(".git/config"), "[core]\n").unwrap();
        let bytes = build_bundle_zip(&d).unwrap();
        let z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let names: Vec<String> = z
            .file_names()
            .filter_map(|n| n.ok().map(|s| s.into_owned()))
            .collect();
        assert!(names.contains(&"a.md".to_string()));
        assert!(!names
            .iter()
            .any(|n| n.contains(".env") || n.contains(".git")));
    }

    #[test]
    fn vendored_okf_viewer_assets_are_complete() {
        // Coverage lock (20-04): the vendored official viewer assets must be
        // present and the template must keep the placeholders plan 20-05
        // substitutes at export time.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/okf-viz");
        for name in [
            "viz-template.html",
            "viz.css",
            "viz.js",
            "cytoscape.min.js",
            "marked.min.js",
        ] {
            assert!(dir.join(name).is_file(), "missing vendored asset {name}");
        }
        let template = std::fs::read_to_string(dir.join("viz-template.html")).unwrap();
        for token in [
            "__BUNDLE_NAME__",
            "__BUNDLE_DATA__",
            "__VIZ_CSS__",
            "__VIZ_JS__",
        ] {
            assert!(template.contains(token), "template lost {token}");
        }
        // Apache-2.0 attribution header retained (Google LLC).
        assert!(template.contains("Copyright 2026 Google LLC"));
        // The two CDN libraries the template references (20-05 inlines them).
        assert!(template.contains("cytoscape@3.28.1"));
        assert!(template.contains("marked@12.0.0"));
    }

    #[test]
    fn vendored_okf_viewer_provenance_checksums_match() {
        // Tamper-evidence lock (20-04): every vendored asset must match the
        // sha256 recorded in PROVENANCE.md's provenance table.
        use sha2::{Digest, Sha256};
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/okf-viz");
        let record = std::fs::read_to_string(dir.join("PROVENANCE.md")).unwrap();
        // Only the provenance table (after its heading) carries hashes;
        // the license table above has rows starting with the same file names.
        let record = record.split("## Provenance").nth(1).unwrap();
        let mut checked = 0;
        for name in [
            "viz-template.html",
            "viz.css",
            "viz.js",
            "cytoscape.min.js",
            "marked.min.js",
        ] {
            let needle = format!("| `{name}` |");
            let line = record
                .lines()
                .find(|l| l.starts_with(&needle))
                .unwrap_or_else(|| panic!("PROVENANCE.md has no row for {name}"));
            // Row shape: `| \`file\` | url | \`sha256\` |` → cell index 3.
            let hash = line.split('|').nth(3).unwrap().trim().trim_matches('`');
            let bytes = std::fs::read(dir.join(name)).unwrap();
            let actual = format!("{:x}", Sha256::digest(&bytes));
            assert_eq!(actual, hash, "sha256 drift for {name}");
            checked += 1;
        }
        assert_eq!(checked, 5);
    }

    // ---------- 20-05: synthesized viz.html entry (OKFE-02) ----------

    #[test]
    fn bundle_contains_synthesized_viz() {
        let d = tmpdir("viz-synth");
        page(
            &d,
            "notes/a.md",
            "---\ntype: concept\ntitle: Alpha\ndescription: first page\ntags: [x]\n---\n\nAlpha body links [B](b.md).\n",
        );
        page(&d, "notes/b.md", &typed("Beta"));
        let bytes = build_bundle_zip(&d).unwrap();
        let names = zip_names(&bytes);
        assert!(names.contains(&"viz.html".to_string()), "{names:?}");
        let viz = String::from_utf8(zip_entry(&bytes, "viz.html")).unwrap();
        // Concept ids are bundle-relative paths minus ".md".
        assert!(viz.contains("notes/a"), "viz lacks concept a");
        assert!(viz.contains("notes/b"), "viz lacks concept b");
        // Bundle name = source dir file name (no slug param exists).
        let bundle_name = d.file_name().unwrap().to_string_lossy().into_owned();
        assert!(viz.contains(&bundle_name), "viz lacks bundle name");
        // Fully offline: CDN tags replaced by inlined library bytes.
        assert!(!viz.contains("cdn.jsdelivr.net"), "CDN reference leaked");
        assert!(viz.contains("cytoscape"), "cytoscape not inlined");
        assert!(viz.contains("marked"), "marked not inlined");
        // Injection happened: payload present, placeholder tokens consumed.
        assert!(viz.contains("window.BUNDLE ="));
        assert!(!viz.contains("__BUNDLE_DATA__"), "placeholder not replaced");
        assert!(!viz.contains("__BUNDLE_NAME__"), "placeholder not replaced");
        // In-memory only, like the index.md precedent: never written to disk.
        assert!(!d.join("viz.html").exists());
    }

    #[test]
    fn vault_viz_wins() {
        // Collision policy (Phase 18 D-06 precedent): the vault's own
        // viz.html ships byte-identical; synthesis is only-when-absent.
        let d = tmpdir("viz-collision");
        page(&d, "a.md", &typed("A"));
        page(&d, "viz.html", "SENTINEL-VIZ\n");
        let bytes = build_bundle_zip(&d).unwrap();
        assert_eq!(zip_entry(&bytes, "viz.html"), b"SENTINEL-VIZ\n");
        // Same entry count as the equivalent vault that gets a synthesized
        // viz.html — one viz entry either way, never two.
        let d2 = tmpdir("viz-collision-b");
        page(&d2, "a.md", &typed("A"));
        let bytes2 = build_bundle_zip(&d2).unwrap();
        assert_eq!(zip_names(&bytes).len(), zip_names(&bytes2).len());
    }

    #[test]
    fn viz_bytes_count_toward_caps() {
        // T-18-10: the guard is consulted with (entries + 1, bytes + viz_len)
        // BEFORE the entry is written — same semantics build_bundle_zip uses.
        let viz_len = 1_000_000u64;
        // Running total + viz bytes crosses the cap → the guard must reject.
        let r = check_export_caps(1, (MAX_EXPORT_BYTES - viz_len / 2) + viz_len);
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("bundle too large"));
        // A viz-sized entry under the cap is fine.
        assert!(check_export_caps(0, viz_len).is_ok());
    }

    #[test]
    fn viz_escapes_script_breakout() {
        // Stored-XSS guard: vault-controlled text inside the <script> block
        // must not be able to close it. `<` is escaped as \u003c in the JSON.
        let d = tmpdir("viz-xss");
        page(
            &d,
            "a.md",
            "---\ntype: concept\ntitle: </script><b>x</b>\n---\n\nbody\n",
        );
        let viz = String::from_utf8(zip_entry(&build_bundle_zip(&d).unwrap(), "viz.html")).unwrap();
        assert!(!viz.contains("</script><b>x</b>"), "script breakout leaked");
        assert!(
            viz.contains("\\u003c/script\\u003e<b>x\\u003c/b\\u003e"),
            "escaped form missing"
        );
    }

    #[test]
    fn export_caps_guard_direct() {
        // Constants pinned (T-18-10): 20 000 entries / 256 MiB.
        assert_eq!(MAX_EXPORT_ENTRIES, 20_000);
        assert_eq!(MAX_EXPORT_BYTES, 268_435_456);
        assert!(check_export_caps(MAX_EXPORT_ENTRIES - 1, MAX_EXPORT_BYTES - 1).is_ok());
        assert!(check_export_caps(MAX_EXPORT_ENTRIES, 0).is_err());
        assert!(check_export_caps(0, MAX_EXPORT_BYTES).is_err());
    }
}
