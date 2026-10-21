//! Admin directory picker logic (Phase 7, REQ-UX-001..007).
//!
//! Kept free of axum so the confinement rules are unit-testable. The handler
//! in `server.rs` only maps these results to JSON.
//!
//! **Confinement**: an admin picks where a vault lives on THIS machine, so the
//! picker is rooted at the configured data dir. Browsing `/etc` or `/home`
//! is refused (`..`, absolute escapes outside the root).

use std::path::{Component, Path, PathBuf};

/// One directory entry in a browse response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    /// True when the directory looks like an mdBook (has `book.toml`).
    pub is_vault: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseListing {
    pub current: String,
    pub parent: Option<String>,
    pub dirs: Vec<DirEntry>,
}

/// Why a browse request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseError {
    /// The requested path is not under the configured root.
    OutsideRoot,
    /// The path does not exist, or is not a directory.
    NotADirectory,
    /// The path could not be read (permissions, I/O).
    Unreadable,
}

impl BrowseError {
    pub fn as_status(self) -> u16 {
        match self {
            // 403: authenticated as admin, but not allowed to go there.
            BrowseError::OutsideRoot => 403,
            BrowseError::NotADirectory => 404,
            BrowseError::Unreadable => 500,
        }
    }
}

/// Normalize a client-supplied path **without** touching the filesystem.
///
/// Percent-decodes first (so `%2e%2e` cannot hide a `..`), then resolves
/// `.`/`..` lexically. Absolute inputs are treated as root-relative so a
/// picker starting at `/data` cannot be walked up to `/`.
pub fn normalize_input(raw: &str) -> PathBuf {
    normalize_input_reporting_escape(raw).0
}

/// Like [`normalize_input`] but also reports whether the input tried to climb
/// above the start. Callers MUST surface that as a refusal rather than serving
/// the clamped path: silently turning an escape attempt into a 404 hides it
/// from the admin and from the logs.
pub fn normalize_input_reporting_escape(raw: &str) -> (PathBuf, bool) {
    // Decode exactly once, here. Everything downstream (containment check, the
    // filesystem path we build) then works on the SAME string.
    let decoded = percent_decode(raw);
    let trimmed = decoded.trim_start_matches('/');

    let mut out = PathBuf::new();
    let mut escaped = false;
    for seg in trimmed.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                // PathBuf::pop returns false when there was nothing to pop —
                // i.e. the input climbed above the start.
                if !out.pop() {
                    escaped = true;
                }
            }
            s => out.push(s),
        }
    }
    (out, escaped)
}

/// Percent-decode a path (`%2e` -> `.`, `%2f` -> `/`). Malformed sequences are
/// left verbatim.
///
/// NOT used on the request path: axum's `Query` extractor decodes for us. Kept
/// for the output side, where we build the encoded paths we hand to the client.
/// Percent-DECODE a query value (`%20` -> space). Malformed sequences are left
/// verbatim.
///
/// axum's `Query` extractor does NOT percent-decode values (it only decodes the
/// path), so a client sending `?path=/my%20notes` hands us the literal `%20`.
/// Decoding happens exactly here, once, before any traversal analysis — never
/// twice, or the checked string differs from the one used to build the path.
pub fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

pub fn percent_encode_path(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// True when `candidate` is `root` itself or lives under it.
pub fn is_within(root: &Path, candidate: &Path) -> bool {
    candidate == root || candidate.starts_with(root)
}

/// Build the listing for a client-supplied `path`, confined to `root`.
///
/// `root` should be canonicalized by the caller so the comparison is fair.
pub fn browse(root: &Path, raw_path: &str) -> Result<BrowseListing, BrowseError> {
    let (rel, escaped) = normalize_input_reporting_escape(raw_path);
    if escaped {
        return Err(BrowseError::OutsideRoot);
    }
    let target = root.join(&rel);

    // Defence in depth: even after lexical normalization, verify containment.
    if !is_within(root, &target) {
        return Err(BrowseError::OutsideRoot);
    }
    // Reject a symlink that escapes the root (lexical checks cannot see this).
    if let Ok(canon) = std::fs::canonicalize(&target) {
        if let Ok(root_canon) = std::fs::canonicalize(root) {
            if !is_within(&root_canon, &canon) {
                return Err(BrowseError::OutsideRoot);
            }
        }
    }

    let meta = std::fs::metadata(&target).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => BrowseError::NotADirectory,
        _ => BrowseError::Unreadable,
    })?;
    if !meta.is_dir() {
        return Err(BrowseError::NotADirectory);
    }

    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(&target).map_err(|_| BrowseError::Unreadable)? {
        let Ok(entry) = entry else { continue };
        // Hidden entries are noise in a picker; never offer them.
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_meta) = entry.metadata() else {
            continue;
        };
        if !file_meta.is_dir() {
            continue;
        }
        let is_vault = entry.path().join("book.toml").is_file();
        dirs.push(DirEntry {
            name: name.clone(),
            path: percent_encode_path(&child_path(&rel.to_string_lossy(), &name)),
            is_vault,
        });
    }
    dirs.sort_by(|a, b| {
        // Vaults first (they are the likely target), then alphabetical.
        b.is_vault
            .cmp(&a.is_vault)
            .then_with(|| a.name.cmp(&b.name))
    });

    // Derive from `rel` (already percent-decoded by normalize_input), NOT from
    // raw_path — otherwise an encoded input echoes back its %20 verbatim.
    let current = percent_encode_path(&display_path(&rel.to_string_lossy()));
    let parent = {
        let up = &rel.parent().map(Path::to_path_buf);
        match up {
            Some(p) if !p.as_os_str().is_empty() => {
                Some(percent_encode_path(&display_path(&p.to_string_lossy())))
            }
            _ => None,
        }
    };

    Ok(BrowseListing {
        current,
        parent,
        dirs,
    })
}

/// Join a child name onto the raw path, preserving the caller's leading `/`.
fn child_path(raw: &str, name: &str) -> String {
    // Work on the DECODED base, then encode the whole result once, so we never
    // double-encode or mix an encoded base with a raw name.
    let base = display_path(raw);
    let joined = if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    };
    joined
}

/// Normalized, always-slash-prefixed display form of a path.
fn display_path(raw: &str) -> String {
    let norm = normalize_input(raw);
    let s = norm.to_string_lossy();
    if s.is_empty() {
        "/".to_string()
    } else {
        format!("/{s}")
    }
}

/// Reject a client-supplied destination that is not a usable vault path.
pub fn validate_vault_path(raw: &str) -> Result<PathBuf, &'static str> {
    let p = Path::new(raw);
    if p.as_os_str().is_empty() {
        return Err("path is empty");
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("path must not contain '..'");
    }
    Ok(p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("morinet-browse-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn normalize_strips_root_and_dots() {
        assert_eq!(normalize_input(""), PathBuf::new());
        assert_eq!(normalize_input("/"), PathBuf::new());
        assert_eq!(
            normalize_input("/data/vaults"),
            PathBuf::from("data/vaults")
        );
        assert_eq!(
            normalize_input("data//vaults/"),
            PathBuf::from("data/vaults")
        );
        assert_eq!(normalize_input("/./data/./x"), PathBuf::from("data/x"));
    }

    #[test]
    fn normalize_cannot_escape_above_root() {
        // Popping past the start yields the root-relative result, never "..".
        assert_eq!(normalize_input("/../../etc"), PathBuf::from("etc"));
        assert_eq!(normalize_input("/data/../../../etc"), PathBuf::from("etc"));
        assert_eq!(normalize_input("../.."), PathBuf::new());
    }

    #[test]
    fn normalize_decodes_query_values_once() {
        // axum does NOT decode query values: %20 arrives raw and must be
        // decoded here, exactly once.
        assert_eq!(normalize_input("/my%20notes"), PathBuf::from("my notes"));
        assert_eq!(normalize_input("/a b/c"), PathBuf::from("a b/c"));
        assert_eq!(normalize_input("/caf%C3%A9"), PathBuf::from("café"));
    }

    #[test]
    fn decoding_happens_exactly_once() {
        // %2520 is an encoded "%20": one decode must leave the literal "%20",
        // not a space. Double-decoding here would be the bug.
        assert_eq!(normalize_input("/a%2520b"), PathBuf::from("a%20b"));
    }

    #[test]
    fn normalize_refuses_literal_escapes_in_the_decoded_input() {
        // If the framework decoded %2e%2e%2f, we see "../" — and must refuse.
        assert_eq!(normalize_input("/../etc"), PathBuf::from("etc"));
        let (p, escaped) = normalize_input_reporting_escape("/../../etc");
        assert!(escaped, "escape detected after decoding");
        assert_eq!(p, PathBuf::from("etc"));
    }

    #[test]
    fn percent_encode_handles_spaces_and_accents() {
        assert_eq!(percent_encode_path("/my notes"), "/my%20notes");
        assert_eq!(percent_encode_path("/café"), "/caf%C3%A9");
        assert_eq!(percent_encode_path("/plain-path_1.0"), "/plain-path_1.0");
    }

    #[test]
    fn encode_then_decode_is_a_round_trip() {
        // What we emit must decode back to exactly what we inspected.
        for raw in ["/my notes", "/café", "/a#b?c", "/plain"] {
            let encoded = percent_encode_path(raw);
            let decoded = urldecode(&encoded);
            assert_eq!(decoded, raw, "round trip for {raw} (encoded {encoded})");
        }
    }

    /// Minimal decoder used only to assert the round-trip property in tests.
    fn urldecode(raw: &str) -> String {
        let bytes = raw.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn is_within_accepts_root_and_children() {
        let root = Path::new("/data");
        assert!(is_within(root, Path::new("/data")));
        assert!(is_within(root, Path::new("/data/x/y")));
        assert!(!is_within(root, Path::new("/etc")));
        assert!(!is_within(root, Path::new("/")));
    }

    #[test]
    fn browse_lists_directories_with_vault_flag() {
        let root = tmp("root");
        std::fs::create_dir_all(root.join("alpha")).unwrap();
        std::fs::create_dir_all(root.join("beta")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("alpha/book.toml"), "[book]").unwrap();
        std::fs::write(root.join("loose.txt"), "x").unwrap();

        let out = browse(&root, "/").expect("root listing");
        let names: Vec<&str> = out.dirs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta"], "hidden + files excluded");
        assert!(out.dirs[0].is_vault, "book.toml flagged");
        assert!(!out.dirs[1].is_vault);
        assert_eq!(out.current, "/");
        assert_eq!(out.parent, None, "root has no parent");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn browse_descends_and_reports_parent() {
        let root = tmp("descend");
        std::fs::create_dir_all(root.join("data/vaults/demo/src")).unwrap();
        let out = browse(&root, "/data/vaults").expect("listing");
        assert_eq!(out.current, "/data/vaults");
        assert_eq!(out.parent.as_deref(), Some("/data"));
        assert_eq!(out.dirs.len(), 1);
        assert_eq!(out.dirs[0].name, "demo");
        assert_eq!(out.dirs[0].path, "/data/vaults/demo");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn browse_refuses_traversal_outside_root() {
        let root = tmp("escape");
        // Climbing above the start is a refusal, not a clamped 404.
        for attack in [
            "/../../etc",
            "/data/../../../etc",
            "/../etc",
            "/%2e%2e%2fetc",
        ] {
            assert_eq!(
                browse(&root, attack).unwrap_err(),
                BrowseError::OutsideRoot,
                "{attack} must be refused"
            );
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn escaping_input_is_reported_not_silently_clamped() {
        // A leading `..` is still visible in the clamped path, but the flag
        // must be set so callers can refuse.
        let (p, escaped) = normalize_input_reporting_escape("/../../etc");
        assert!(escaped, "escape reported");
        assert_eq!(p, PathBuf::from("etc"), "clamped for display only");
        let (_, not_escaped) = normalize_input_reporting_escape("/data/vaults");
        assert!(!not_escaped, "normal path is not an escape");
    }

    #[test]
    fn browse_reports_missing_and_file_paths() {
        let root = tmp("missing");
        std::fs::write(root.join("f.txt"), "x").unwrap();
        assert_eq!(
            browse(&root, "/nope").unwrap_err(),
            BrowseError::NotADirectory
        );
        assert_eq!(
            browse(&root, "/f.txt").unwrap_err(),
            BrowseError::NotADirectory
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn error_statuses_are_sane() {
        assert_eq!(BrowseError::OutsideRoot.as_status(), 403);
        assert_eq!(BrowseError::NotADirectory.as_status(), 404);
        assert_eq!(BrowseError::Unreadable.as_status(), 500);
    }

    #[test]
    fn validate_vault_path_rejects_traversal_and_empty() {
        assert!(validate_vault_path("/data/vaults/demo").is_ok());
        assert!(validate_vault_path("").is_err());
        assert!(validate_vault_path("/data/../etc").is_err());
    }

    #[test]
    fn vault_paths_sort_before_plain_directories() {
        let root = tmp("sorting");
        for name in ["aaa", "zzz"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        std::fs::create_dir_all(root.join("mmm")).unwrap();
        std::fs::write(root.join("mmm/book.toml"), "[book]").unwrap();
        let out = browse(&root, "/").expect("listing");
        assert_eq!(out.dirs[0].name, "mmm", "vault first regardless of alpha");
        std::fs::remove_dir_all(&root).ok();
    }
}
