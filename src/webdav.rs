//! WebDAV surface (Phase 6, REQ-WEBDAV-*).
//!
//! AD-5: hand-written axum handlers, no crate — the spec slice we support is
//! tiny (PROPFIND depth 0/1, GET, admin PUT).
//!
//! **What is exposed**: the vault's *source* directory (`book.toml` + `src/`),
//! not the compiled `book/`. That is the only shape that makes WebDAV useful
//! for a second brain: you PUT markdown, and mdBook renders it. The store
//! keeps the compiled dir (`<src>/book`), so the source is its parent.
//!
//! Auth is HTTP Basic against the same bcrypt users table as the web login,
//! gated by the same per-vault ACL. PUT is admin-only.

use std::path::{Path, PathBuf};

use base64::Engine;
use std::time::SystemTime;

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

/// Strong ETag for raw source bytes: the QUOTED wire form `"sha256-hex"`.
///
/// Phase 15 (CONF-01) "one hash, three consumers": GET emission, PROPFIND
/// `getetag`, and the 15-03/15-04 precondition checks all call this single
/// helper — it is the one source of truth for both hashing AND quoting, so
/// comparisons use the quoted form directly. Strong validator only: never
/// `W/`, never mtime (PITFALLS.md C1/C2 — hash the source layer, not the
/// build output).
pub fn etag_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("\"{}\"", hex::encode(h.finalize()))
}

/// The ONE shared "write source file" helper (ROADMAP criterion 7,
/// PITFALLS C3/C5): writes `bytes` to a sibling temp file `.{name}.tmp-{uuid}`
/// in the SAME directory (same filesystem under `data_dir`, so the rename is
/// atomic), then renames it onto `path`. On any error the temp file is removed
/// and the error propagates — no truncated or interleaved targets, ever.
/// Returns the quoted ETag of the bytes written (recomputed from the write
/// payload per C3, never from a pre-write hash). Callers hold the per-vault
/// write lock across their check→write section (see `vault_lock` in server.rs);
/// 15-04's upload path reuses this same helper.
pub async fn write_source_atomic(
    path: &std::path::Path,
    bytes: &[u8],
) -> anyhow::Result<String> {
    use tokio::io::AsyncWriteExt;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("write target has no file name"))?
        .to_string_lossy()
        .to_string();
    let tmp = path.with_file_name(format!(".{file_name}.tmp-{}", uuid::Uuid::new_v4()));
    let result: anyhow::Result<()> = async {
        let mut f = tokio::fs::File::create(&tmp).await?;
        f.write_all(bytes).await?;
        f.flush().await?;
        drop(f);
        tokio::fs::rename(&tmp, path).await?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    result?;
    Ok(etag_of(bytes))
}

/// Resolve the vault source dir from the stored compiled-book dir.
/// mdBook always writes `<src>/book`, so the parent is the source root.
pub fn source_dir_of(book_path: &Path) -> Option<PathBuf> {
    let p = book_path.to_path_buf();
    if p.file_name()? != "book" {
        // Not a standard mdbook output dir — the stored path IS the source
        // (a plain static dir is registered as-is).
        return Some(p);
    }
    let parent = p.parent()?;
    // llm-wiki layout: <source>/.morinet-build/book — the generated tree is
    // never a source; WebDAV must write into the vault the user owns (AD-13).
    if parent
        .file_name()
        .map(|n| n == ".morinet-build")
        .unwrap_or(false)
    {
        return parent.parent().map(Path::to_path_buf);
    }
    Some(parent.to_path_buf())
}

/// Reject `..` segments and absolute paths, then join under `root`.
/// Returns `None` when the request tries to escape the vault.
pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.trim_start_matches('/');
    let rel = rel.split('?').next().unwrap_or("");
    if rel.split('/').any(|c| c == ".." || c == ".") {
        return None;
    }
    if rel.contains('\0') {
        return None;
    }
    Some(root.join(rel))
}

/// RFC3339 timestamp for a WebDAV `<D:propstat>` getlastmodified.
fn http_date(t: SystemTime) -> String {
    // RFC 7231 IMF-fixdate: "Tue, 14 Nov 2023 22:13:20 GMT".
    // No chrono: civil-from-days (Hinnant) + the epoch-1970-was-a-Thursday
    // offset for the weekday. Verified against Python for several instants.
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (h, mi, sec) = (tod / 3600, (tod % 3600) / 60, tod % 60);

    const DOW: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // 1970-01-01 was a Thursday => index 4 in a Sunday-first table.
    let dow = DOW[(days.unsigned_abs() as usize + 4) % 7];

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        dow,
        d,
        MON[(m - 1) as usize],
        year,
        h,
        mi,
        sec
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Percent-encode a path segment for href output (WebDAV hrefs are URIs).
fn href_encode(seg: &str) -> String {
    let mut out = String::with_capacity(seg.len());
    for b in seg.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn href_for(base: &str, rel: &str, is_dir: bool) -> String {
    let mut h = String::from(base.trim_end_matches('/'));
    for (i, seg) in rel.split('/').filter(|s| !s.is_empty()).enumerate() {
        h.push('/');
        h.push_str(&href_encode(seg));
        let _ = i;
    }
    if is_dir && !h.ends_with('/') {
        h.push('/');
    }
    h
}

/// One entry in a PROPFIND multistatus response. `etag` is the quoted wire
/// form from [`etag_of`]; `Some` only for files (CONF-01: collections have
/// no getetag).
fn prop_entry(
    href: String,
    is_dir: bool,
    size: u64,
    modified: SystemTime,
    etag: Option<&str>,
) -> String {
    let displayname = href
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    let resourcetype = if is_dir { "<D:collection/>" } else { "" };
    let getetag = etag
        .map(|e| format!("      <D:getetag>{e}</D:getetag>\n"))
        .unwrap_or_default();
    format!(
        r#"<D:response>
  <D:href>{href}</D:href>
  <D:propstat>
    <D:prop>
      <D:resourcetype>{rt}</D:resourcetype>
      <D:displayname>{displayname}</D:displayname>
      <D:getcontentlength>{size}</D:getcontentlength>
      <D:getlastmodified>{lm}</D:getlastmodified>
      <D:getcontenttype>{ct}</D:getcontenttype>
{getetag}    </D:prop>
    <D:status>HTTP/1.1 200 OK</D:status>
  </D:propstat>
</D:response>"#,
        href = xml_escape(&href),
        rt = resourcetype,
        displayname = xml_escape(&displayname),
        size = size,
        lm = http_date(modified),
        ct = if is_dir {
            "httpd/unix-directory"
        } else {
            "text/plain"
        },
    )
}

/// A directory listing, sorted (collections first) as most clients expect.
pub struct Entry {
    pub rel: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: SystemTime,
    /// Quoted strong ETag from [`etag_of`] for files; `None` for collections.
    pub etag: Option<String>,
}

/// Build a `<D:multistatus>` body for `depth: 0` (just the collection) or
/// `depth: 1` (collection + direct children).
pub fn multistatus(base: &str, entries: &[Entry]) -> String {
    let mut body = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">"#,
    );
    for e in entries {
        body.push_str(&prop_entry(
            href_for(base, &e.rel, e.is_dir),
            e.is_dir,
            e.size,
            e.modified,
            e.etag.as_deref(),
        ));
    }
    body.push_str("\n</D:multistatus>\n");
    body
}

/// Read one directory's direct children (sorted, dirs first) + the dir itself.
pub async fn list_dir(root: &Path, rel: &str, base: &str, depth_zero_only: bool) -> Option<String> {
    let dir = safe_join(root, rel)?;
    let meta = tokio::fs::metadata(&dir).await.ok()?;
    if !meta.is_dir() {
        return None;
    }
    let mut entries = vec![Entry {
        rel: rel.to_string(),
        is_dir: true,
        size: 0,
        modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        etag: None,
    }];
    if !depth_zero_only {
        let mut rd = tokio::fs::read_dir(&dir).await.ok()?;
        let mut children: Vec<(String, bool, u64, SystemTime, Option<String>)> = Vec::new();
        while let Ok(Some(item)) = rd.next_entry().await {
            let name = item.file_name().to_string_lossy().to_string();
            let child_rel = if rel.trim_end_matches('/').is_empty() {
                name.clone()
            } else {
                format!("{}/{}", rel.trim_end_matches('/'), name)
            };
            let m = match item.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };
            // CONF-01: hash-on-read per file (accepted cost, 15-CONTEXT
            // "Claude's Discretion" — markdown is small, no caching this phase).
            let etag = if m.is_dir() {
                None
            } else {
                tokio::fs::read(item.path()).await.ok().map(|b| etag_of(&b))
            };
            children.push((
                child_rel,
                m.is_dir(),
                m.len(),
                m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                etag,
            ));
        }
        children.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (r, d, s, m, etag) in children {
            entries.push(Entry {
                rel: r,
                is_dir: d,
                size: s,
                modified: m,
                etag,
            });
        }
    }
    Some(multistatus(base, &entries))
}

pub fn propfind_response(xml: String) -> Response {
    (
        StatusCode::MULTI_STATUS,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        Body::from(xml),
    )
        .into_response()
}

/// 207 body for a single resource (used for depth:0 on a file). `etag` is
/// the quoted strong ETag from [`etag_of`] (CONF-01).
pub fn file_propfind_response(
    base: &str,
    rel: &str,
    size: u64,
    modified: SystemTime,
    etag: Option<String>,
) -> Response {
    let entries = vec![Entry {
        rel: rel.to_string(),
        is_dir: false,
        size,
        modified,
        etag,
    }];
    propfind_response(multistatus(base, &entries))
}

/// Percent-decode a URI path segment chain into a plain filesystem-relative
/// path. Decoding BEFORE splitting on `/` is what makes `%2e%2e%2f` and `%2f`
/// visible to the traversal check in [`safe_join`] instead of hiding a
/// separator inside a "single" segment.
pub fn decode_path(raw: &str) -> String {
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
    // A non-UTF8 path is not something we want to serve; fall back to the raw.
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

/// Extract the relative path from a `/webdav/<slug>/...` URI, percent-decoded.
pub fn rel_from_uri(uri: &Uri, slug: &str) -> String {
    let prefix = format!("/webdav/{slug}");
    let p = uri.path().strip_prefix(&prefix).unwrap_or("");
    decode_path(p).trim_start_matches('/').to_string()
}

/// Decide if a WebDAV request is one we handle (vs OPTIONS/HEAD noise).
/// PROPFIND is not in the http crate's Method enum — WebDAV methods arrive as
/// extension methods, so match on the raw token.
pub fn is_propfind(method: &Method) -> bool {
    method.as_str() == "PROPFIND"
}

/// Whether a PUT target may be created (we support plain writes only).
pub fn is_supported(method: &Method) -> bool {
    is_propfind(method) || matches!(*method, Method::GET | Method::PUT)
}

/// Read the `Depth` header (0 or 1; anything else is treated as 0 per RFC).
pub fn depth_of(headers: &HeaderMap) -> u8 {
    match headers
        .get("depth")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim())
    {
        Some("1") => 1,
        _ => 0,
    }
}

/// Parsed state of an `If-Match` / `If-None-Match` precondition header
/// (RFC 9110 §13.1/§13.2). Unlike `depth_of`, preconditions NEVER default on
/// garbage: an unreadable or invalid value is `Malformed` and the caller
/// (15-03) must answer 400 with nothing written — fail closed (PITFALLS C2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precondition {
    /// Header absent — unconditional request (CONF-05 backward compat).
    Absent,
    /// `*` — the wildcard precondition.
    Any,
    /// One or more quoted strong tags. Quotes are KEPT so tags compare
    /// directly against the quoted [`etag_of`] wire form.
    Tags(Vec<String>),
    /// Unreadable (invalid UTF-8) or invalid syntax — caller returns 400.
    Malformed,
}

fn precondition_of(headers: &HeaderMap, name: &str) -> Precondition {
    let Some(value) = headers.get(name) else {
        return Precondition::Absent;
    };
    // An unreadable precondition header is NEVER treated as Absent.
    let Ok(raw) = value.to_str() else {
        return Precondition::Malformed;
    };
    let raw = raw.trim();
    if raw == "*" {
        return Precondition::Any;
    }
    let mut tags = Vec::new();
    for token in raw.split(',') {
        let t = token.trim();
        // Strong validators only: every token must be a plain quoted string.
        // `W/...` weak tags, bare tokens, and empty entries are refused.
        if t.len() < 2
            || !t.starts_with('"')
            || !t.ends_with('"')
            || t[1..t.len() - 1].contains('"')
        {
            return Precondition::Malformed;
        }
        tags.push(t.to_string());
    }
    if tags.is_empty() {
        return Precondition::Malformed;
    }
    Precondition::Tags(tags)
}

/// Parse the `If-Match` header (enforced in 15-03). Pure and total: any
/// input yields a variant, never a panic, never a silent default.
pub fn if_match_of(headers: &HeaderMap) -> Precondition {
    precondition_of(headers, "if-match")
}

/// Parse the `If-None-Match` header (enforced in 15-03). Same fail-closed
/// contract as [`if_match_of`].
pub fn if_none_match_of(headers: &HeaderMap) -> Precondition {
    precondition_of(headers, "if-none-match")
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_dir_is_the_parent_of_book() {
        let p = source_dir_of(Path::new("/data/vaults/demo/book"));
        assert_eq!(p, Some(PathBuf::from("/data/vaults/demo")));
    }

    #[test]
    fn source_dir_unwraps_the_morinet_build_layout() {
        // llm-wiki vault: WebDAV must expose the user's source, NOT the
        // generated tree (<src>/.morinet-build/book → <src>).
        let p = source_dir_of(Path::new("/data/vaults/wiki/.morinet-build/book"));
        assert_eq!(p, Some(PathBuf::from("/data/vaults/wiki")));
    }

    #[test]
    fn non_book_path_is_its_own_source() {
        let p = source_dir_of(Path::new("/data/plain"));
        assert_eq!(p, Some(PathBuf::from("/data/plain")));
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let root = Path::new("/vault");
        assert!(safe_join(root, "../etc/passwd").is_none());
        assert!(safe_join(root, "src/../../etc/passwd").is_none());
        assert!(safe_join(root, "a/..").is_none());
        assert!(
            safe_join(root, "./x").is_none(),
            "dot segments rejected too"
        );
    }

    #[test]
    fn safe_join_accepts_nested_paths() {
        let root = Path::new("/vault");
        assert_eq!(
            safe_join(root, "src/note.md"),
            Some(PathBuf::from("/vault/src/note.md"))
        );
        assert_eq!(
            safe_join(root, "/src/note.md"),
            Some(PathBuf::from("/vault/src/note.md"))
        );
        assert_eq!(safe_join(root, ""), Some(PathBuf::from("/vault")));
    }

    #[test]
    fn rel_from_uri_strips_the_slug_prefix() {
        let uri: Uri = "/webdav/demo/src/home.md".parse().unwrap();
        assert_eq!(rel_from_uri(&uri, "demo"), "src/home.md");
        let root: Uri = "/webdav/demo".parse().unwrap();
        assert_eq!(rel_from_uri(&root, "demo"), "");
    }

    #[test]
    fn href_is_percent_encoded_and_slash_terminated_for_dirs() {
        assert_eq!(
            href_for("/webdav/demo", "src/a b.md", false),
            "/webdav/demo/src/a%20b.md"
        );
        assert_eq!(href_for("/webdav/demo", "src", true), "/webdav/demo/src/");
        assert_eq!(href_for("/webdav/demo", "", true), "/webdav/demo/");
    }

    #[test]
    fn multistatus_marks_dirs_and_sorts_them_first() {
        let entries = vec![
            Entry {
                rel: "".into(),
                is_dir: true,
                size: 0,
                modified: SystemTime::UNIX_EPOCH,
                etag: None,
            },
            Entry {
                rel: "src".into(),
                is_dir: true,
                size: 0,
                modified: SystemTime::UNIX_EPOCH,
                etag: None,
            },
            Entry {
                rel: "README.md".into(),
                is_dir: false,
                size: 12,
                modified: SystemTime::UNIX_EPOCH,
                etag: Some(etag_of(b"hello")),
            },
        ];
        let xml = multistatus("/webdav/demo", &entries);
        assert!(xml.starts_with("<?xml"));
        assert!(xml.contains("<D:multistatus xmlns:D=\"DAV:\">"));
        assert!(xml.contains("<D:collection/>"), "dirs flagged");
        assert!(xml.contains("<D:getcontentlength>12</D:getcontentlength>"));
        assert!(
            xml.contains(&format!("<D:getetag>{}</D:getetag>", etag_of(b"hello"))),
            "file entry carries getetag"
        );
        assert_eq!(
            xml.matches("<D:getetag>").count(),
            1,
            "collections carry no getetag"
        );
        assert!(xml.contains("HTTP/1.1 200 OK"));
        assert!(xml.ends_with("</D:multistatus>\n"));
    }

    #[test]
    fn etag_of_is_strong_sha256() {
        // CONF-01: strong validator = quoted sha256-hex of the raw bytes.
        // Known digest of b"hello".
        let etag = etag_of(b"hello");
        assert_eq!(
            etag,
            "\"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\""
        );
        assert!(
            etag.starts_with('"') && etag.ends_with('"'),
            "quoted wire form"
        );
        let hex_body = &etag[1..etag.len() - 1];
        assert_eq!(hex_body.len(), 64, "sha256 hex is 64 chars");
        assert!(
            hex_body
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "lowercase hex only"
        );
        assert!(!etag.contains("W/"), "strong validators only (C2)");
    }

    /// Shared fail-closed table for both precondition parsers (RFC 9110
    /// §13.1/§13.2). Values are raw header bytes so the non-UTF-8 case can
    /// build a HeaderValue that `to_str()` refuses.
    fn precondition_table() -> Vec<(Vec<u8>, Precondition)> {
        vec![
            (
                b"\"abc\"".to_vec(),
                Precondition::Tags(vec!["\"abc\"".into()]),
            ),
            (
                b"  \"abc\" , \"def\"  ".to_vec(),
                Precondition::Tags(vec!["\"abc\"".into(), "\"def\"".into()]),
            ),
            (b"*".to_vec(), Precondition::Any),
            (b" * ".to_vec(), Precondition::Any),
            // Weak validators are never accepted (PITFALLS C2):
            (b"W/\"abc\"".to_vec(), Precondition::Malformed),
            (b"abc".to_vec(), Precondition::Malformed),
            (b"".to_vec(), Precondition::Malformed),
            (b"\"abc\", W/\"def\"".to_vec(), Precondition::Malformed),
            // Invalid UTF-8 header bytes: fail closed, NEVER Absent.
            (vec![0xff, 0xfe], Precondition::Malformed),
        ]
    }

    fn headers_with(name: &str, raw: &[u8]) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            axum::http::HeaderName::try_from(name).unwrap(),
            axum::http::HeaderValue::from_bytes(raw).unwrap(),
        );
        h
    }

    #[test]
    fn if_match_parses_per_rfc9110() {
        assert_eq!(if_match_of(&HeaderMap::new()), Precondition::Absent);
        for (raw, want) in precondition_table() {
            assert_eq!(
                if_match_of(&headers_with("if-match", &raw)),
                want,
                "If-Match: {raw:?}"
            );
        }
    }

    #[test]
    fn if_none_match_parses_per_rfc9110() {
        assert_eq!(if_none_match_of(&HeaderMap::new()), Precondition::Absent);
        for (raw, want) in precondition_table() {
            assert_eq!(
                if_none_match_of(&headers_with("if-none-match", &raw)),
                want,
                "If-None-Match: {raw:?}"
            );
        }
    }

    #[test]
    fn http_date_formats_a_known_instant() {
        // 2023-11-14T22:13:20Z
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        assert_eq!(http_date(t), "Tue, 14 Nov 2023 22:13:20 GMT");
    }

    #[tokio::test]
    async fn list_dir_depth0_returns_only_the_collection() {
        let dir = std::env::temp_dir().join(format!("morinet-dav0-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.md"), "hi").unwrap();
        let xml = list_dir(&dir, "", "/webdav/demo", true)
            .await
            .expect("listing");
        assert!(xml.contains("</D:multistatus>"));
        assert!(!xml.contains("a.md"), "depth 0 hides children");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn list_dir_depth1_includes_children_with_dirs_first() {
        let dir = std::env::temp_dir().join(format!("morinet-dav1-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/a.md"), "hello").unwrap();
        std::fs::write(dir.join("book.toml"), "x").unwrap();

        // depth 1 lists the collection + its DIRECT children only.
        let xml = list_dir(&dir, "", "/webdav/demo", false)
            .await
            .expect("listing");
        assert!(xml.contains("src/"), "child dir listed");
        assert!(xml.contains("book.toml"), "child file listed");
        assert!(
            !xml.contains("a.md"),
            "grandchildren are not listed at depth 1"
        );
        // dirs before files
        let i_src = xml.find("src/").unwrap();
        let i_book = xml.find("book.toml").unwrap();
        assert!(i_src < i_book, "collection sorts before file");

        // depth 1 on the subdir lists its own children.
        let xml1 = list_dir(&dir, "src", "/webdav/demo", false)
            .await
            .expect("subdir listing");
        assert!(xml1.contains("a.md"));
        assert!(!xml1.contains("book.toml"), "subdir listing is scoped");
        std::fs::remove_dir_all(&dir).ok();
    }
}

// ---------- Basic auth (HTTP Basic against the same users table) ----------

/// Decode an `Authorization: Basic <b64>` header → (user, password).
pub fn parse_basic(header_value: &str) -> Option<(String, String)> {
    let raw = header_value
        .strip_prefix("Basic ")
        .or(header_value.strip_prefix("basic "))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    // Split on the FIRST colon: passwords may contain colons.
    let (user, pass) = text.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// RFC 7235 challenge header for a 401.
pub fn www_authenticate_basic() -> (header::HeaderName, &'static str) {
    (
        header::WWW_AUTHENTICATE,
        r#"Basic realm="MoriNet WebDAV", charset="UTF-8""#,
    )
}

#[cfg(test)]
mod auth_tests {
    use super::*;

    #[test]
    fn basic_auth_decodes_user_and_password() {
        // "alice:s3cret" base64
        let h = "Basic YWxpY2U6czNjcmV0";
        assert_eq!(
            parse_basic(h),
            Some(("alice".to_string(), "s3cret".to_string()))
        );
    }

    #[test]
    fn basic_auth_keeps_colons_in_password() {
        // "bob:pa:ss:word"
        let raw = base64::engine::general_purpose::STANDARD.encode("bob:pa:ss:word");
        assert_eq!(
            parse_basic(&format!("Basic {raw}")),
            Some(("bob".to_string(), "pa:ss:word".to_string()))
        );
    }

    #[test]
    fn basic_auth_rejects_garbage_and_other_schemes() {
        assert_eq!(parse_basic("Bearer abc123"), None);
        assert_eq!(parse_basic("Basic !!!!not-base64!!!"), None);
        assert_eq!(parse_basic("Basic bm8tY29sb24taGVyZQ=="), None); // no colon
        assert_eq!(parse_basic(""), None);
    }

    #[test]
    fn challenge_is_a_basic_realm() {
        let (name, value) = www_authenticate_basic();
        assert_eq!(name, header::WWW_AUTHENTICATE);
        assert!(value.starts_with("Basic realm="));
    }
}

#[cfg(test)]
mod decode_tests {
    use super::*;

    #[test]
    fn percent_encoding_is_decoded() {
        assert_eq!(decode_path("/src/a%20b.md"), "/src/a b.md");
        assert_eq!(decode_path("/src/caf%C3%A9.md"), "/src/café.md");
        assert_eq!(decode_path("/src/plain.md"), "/src/plain.md");
    }

    #[test]
    fn malformed_percent_sequences_are_left_alone() {
        assert_eq!(decode_path("/a%zz"), "/a%zz");
        assert_eq!(decode_path("/a%2"), "/a%2");
        assert_eq!(decode_path("/100%"), "/100%");
    }

    #[test]
    fn encoded_traversal_is_revealed_then_refused() {
        let root = Path::new("/vault");
        // %2e%2e is "..", %2f is "/" — decoding first must expose them.
        for attack in [
            "..%2f..%2fetc%2fpasswd",
            "%2e%2e%2f%2e%2e%2fetc%2fpasswd",
            "src/..%2f..%2fetc%2fpasswd",
        ] {
            let decoded = decode_path(attack);
            assert!(
                safe_join(root, &decoded).is_none(),
                "encoded traversal {attack:?} must be refused (decoded: {decoded:?})"
            );
        }
    }

    #[test]
    fn encoded_separator_cannot_smuggle_a_subpath() {
        // Before the fix this produced a literal file named "..%2fescaped.md".
        let uri: Uri = "/webdav/demo/..%2fescaped.md".parse().unwrap();
        let rel = rel_from_uri(&uri, "demo");
        assert!(rel.contains('/'), "decoded into real segments: {rel}");
        assert!(safe_join(Path::new("/vault"), &rel).is_none(), "refused");
    }

    #[test]
    fn benign_encoded_names_still_resolve() {
        let uri: Uri = "/webdav/demo/src/my%20note.md".parse().unwrap();
        let rel = rel_from_uri(&uri, "demo");
        assert_eq!(
            safe_join(Path::new("/vault"), &rel),
            Some(PathBuf::from("/vault/src/my note.md"))
        );
    }
}
