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

/// Resolve the vault source dir from the stored compiled-book dir.
/// mdBook always writes `<src>/book`, so the parent is the source root.
pub fn source_dir_of(book_path: &Path) -> Option<PathBuf> {
    let p = book_path.to_path_buf();
    if p.file_name()? != "book" {
        // Not a standard mdbook output dir — the stored path IS the source
        // (a plain static dir is registered as-is).
        return Some(p);
    }
    p.parent().map(Path::to_path_buf)
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

/// One entry in a PROPFIND multistatus response.
fn prop_entry(href: String, is_dir: bool, size: u64, modified: SystemTime) -> String {
    let displayname = href
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    let resourcetype = if is_dir { "<D:collection/>" } else { "" };
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
    </D:prop>
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
    }];
    if !depth_zero_only {
        let mut rd = tokio::fs::read_dir(&dir).await.ok()?;
        let mut children: Vec<(String, bool, u64, SystemTime)> = Vec::new();
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
            children.push((
                child_rel,
                m.is_dir(),
                m.len(),
                m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            ));
        }
        children.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (r, d, s, m) in children {
            entries.push(Entry {
                rel: r,
                is_dir: d,
                size: s,
                modified: m,
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

/// 207 body for a single resource (used for depth:0 on a file).
pub fn file_propfind_response(base: &str, rel: &str, size: u64, modified: SystemTime) -> Response {
    let entries = vec![Entry {
        rel: rel.to_string(),
        is_dir: false,
        size,
        modified,
    }];
    propfind_response(multistatus(base, &entries))
}

/// Extract the relative path from a `/webdav/<slug>/...` URI.
pub fn rel_from_uri(uri: &Uri, slug: &str) -> String {
    let prefix = format!("/webdav/{slug}");
    let p = uri.path().strip_prefix(&prefix).unwrap_or("");
    p.trim_start_matches('/').to_string()
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_dir_is_the_parent_of_book() {
        let p = source_dir_of(Path::new("/data/vaults/demo/book"));
        assert_eq!(p, Some(PathBuf::from("/data/vaults/demo")));
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
            },
            Entry {
                rel: "src".into(),
                is_dir: true,
                size: 0,
                modified: SystemTime::UNIX_EPOCH,
            },
            Entry {
                rel: "README.md".into(),
                is_dir: false,
                size: 12,
                modified: SystemTime::UNIX_EPOCH,
            },
        ];
        let xml = multistatus("/webdav/demo", &entries);
        assert!(xml.starts_with("<?xml"));
        assert!(xml.contains("<D:multistatus xmlns:D=\"DAV:\">"));
        assert!(xml.contains("<D:collection/>"), "dirs flagged");
        assert!(xml.contains("<D:getcontentlength>12</D:getcontentlength>"));
        assert!(xml.contains("HTTP/1.1 200 OK"));
        assert!(xml.ends_with("</D:multistatus>\n"));
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
