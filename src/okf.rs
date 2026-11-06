//! Phase 18 (OKF) — core module: conformance scan, trust tier, note metadata.
//!
//! Pure functions over a directory tree and parsed frontmatter — no store, no
//! async, no new dependencies (D-04/D-10). The conformance rules follow OKF
//! SPEC v0.2 §11 as locked in D-03; trust tiers follow §5.3 (D-07).

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
}
