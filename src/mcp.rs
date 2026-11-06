//! MCP server (Phase 10, PLAN 10-02): StreamableHttp on `/mcp` via rmcp.
//!
//! Auth is a dedicated API token (Bearer, see `Store::user_for_api_token`) —
//! never the password, never the session cookie. Every tool is scoped to the
//! vaults the token's user can access (same ACL as WebDAV), and reads the
//! vault SOURCE (markdown), not the compiled book (AD-13).

use std::path::{Component, Path, PathBuf};

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::store::Store;

/// Reject `..`, absolute paths and any traversal: only plain components join.
pub(crate) fn safe_join(src: &Path, rel: &str) -> Option<PathBuf> {
    if rel.starts_with('/') || Path::new(rel).is_absolute() {
        return None; // absolute paths are refused, not trimmed
    }
    let mut out = src.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            // ParentDir, RootDir, Prefix → refused.
            _ => return None,
        }
    }
    Some(out)
}

/// Sorted relative paths of every `.md` file under `dir`, skipping hidden
/// directories (`.morinet-build`, `.git`, …) — the generated tree is never a
/// source.
pub(crate) fn list_md_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((abs, prefix)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&abs) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let path = e.path();
            if path.is_dir() {
                if !name.starts_with('.') {
                    stack.push((path, rel));
                }
            } else if name.ends_with(".md") {
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

/// One search hit: where and what matched.
#[derive(Debug, serde::Serialize, PartialEq)]
pub struct Hit {
    pub slug: String,
    pub path: String,
    pub line: usize,
    pub snippet: String,
}

/// Case-insensitive substring search across `.md` files of `(slug, dir)`
/// pairs, capped so a huge vault cannot blow the response.
pub(crate) fn search_md(vaults: &[(String, PathBuf)], query: &str, cap: usize) -> Vec<Hit> {
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    'outer: for (slug, dir) in vaults {
        for rel in list_md_files(dir) {
            let Ok(content) = std::fs::read_to_string(dir.join(&rel)) else {
                continue;
            };
            for (i, line) in content.lines().enumerate() {
                if line.to_lowercase().contains(&needle) {
                    let snippet: String = line.trim().chars().take(160).collect();
                    hits.push(Hit {
                        slug: slug.clone(),
                        path: rel.clone(),
                        line: i + 1,
                        snippet,
                    });
                    if hits.len() >= cap {
                        break 'outer;
                    }
                }
            }
        }
    }
    hits
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SlugPath {
    /// Vault slug (from list_vaults).
    pub slug: String,
    /// Path of the markdown note inside the vault, e.g. "concepts/z.md".
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListNotesParams {
    /// Vault slug.
    pub slug: String,
    /// Optional sub-folder inside the vault (defaults to the root).
    #[serde(default)]
    pub path: Option<String>,
    /// Filter: keep only notes whose frontmatter `type` equals this value (exact match).
    #[serde(default)]
    pub r#type: Option<String>,
    /// Filter: keep notes whose frontmatter `tags` contain ANY of these values.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Case-insensitive substring to find in the markdown of accessible vaults.
    pub query: String,
    /// Restrict to one vault slug (defaults to every accessible vault).
    #[serde(default)]
    pub slug: Option<String>,
    /// Filter: keep only notes whose frontmatter `type` equals this value (exact match).
    #[serde(default)]
    pub r#type: Option<String>,
    /// Filter: keep notes whose frontmatter `tags` contain ANY of these values.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// One MCP server instance per request (stateless transport), bound to the
/// user behind the Bearer token.
#[derive(Clone)]
pub struct MoriMcp {
    tool_router: ToolRouter<Self>,
    store: Store,
    email: String,
}

fn tool_error(msg: &str) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::error(vec![ContentBlock::text(
        msg.to_string(),
    )]))
}

/// Post-ACL frontmatter filter (D-08, T-18-15): `slug`/`path` MUST already
/// come from an ACL-gated `crate::api` result — this helper only decides
/// whether an already-accessible note matches `type`/`tags`. No filter ⇒
/// always true; unreadable or frontmatter-less notes never match (T-18-17:
/// `frontmatter_of` returns `None` on any parse failure, no unwrap here).
/// Cost: one extra `read_note` per candidate, bounded by the search cap
/// and per-vault list sizes (T-18-16).
async fn note_matches(
    store: &Store,
    email: &str,
    slug: &str,
    path: &str,
    type_f: Option<&str>,
    tags: Option<&[String]>,
) -> bool {
    if type_f.is_none() && tags.is_none() {
        return true;
    }
    let Ok(bytes) = crate::api::read_note(store, email, slug, path).await else {
        return false;
    };
    let Some(fm) = crate::okf::frontmatter_of(&bytes) else {
        return false;
    };
    type_f.is_none_or(|t| fm.page_type() == Some(t))
        && tags.is_none_or(|want| {
            let have = fm.list("tags");
            want.iter().any(|w| have.contains(w))
        })
}

#[tool_router]
impl MoriMcp {
    pub fn new(store: Store, email: String) -> Self {
        Self {
            tool_router: Self::tool_router(),
            store,
            email,
        }
    }

    /// Best-effort access log: spawn so a slow or failing insert can never
    /// fail the tool call (same contract as the web layer, plan 02).
    fn log_access(&self, slug: &str, path: &str) {
        let store = self.store.clone();
        let email = self.email.clone();
        let slug = slug.to_string();
        let path = path.to_string();
        tokio::spawn(async move {
            let _ = store.log_access(&email, &slug, "mcp", &path).await;
        });
    }

    /// The vaults the token's user can access (personal + shared via groups).
    #[tool(description = "List the vaults this token's user can access")]
    pub async fn list_vaults(&self) -> Result<CallToolResult, ErrorData> {
        let vaults: Vec<serde_json::Value> =
            crate::api::accessible_vault_entries(&self.store, &self.email)
                .await
                .iter()
                .map(crate::api::vault_json)
                .collect();
        self.log_access("-", "-");
        Ok(CallToolResult::success(vec![ContentBlock::text(
            serde_json::to_string(&vaults).unwrap_or_else(|_| "[]".into()),
        )]))
    }

    /// Markdown notes of a vault (source tree, not the compiled book).
    #[tool(description = "List markdown notes of a vault, optionally under a sub-folder")]
    pub async fn list_notes(
        &self,
        Parameters(p): Parameters<ListNotesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Thin wrapper: all ACL + path logic lives in crate::api (D-04).
        match crate::api::list_notes(&self.store, &self.email, &p.slug, p.path.as_deref()).await {
            Ok(files) => {
                // Post-ACL filter (D-08): only notes from accessible vaults
                // are ever examined; no filter ⇒ the list passes through.
                let files = if p.r#type.is_some() || p.tags.is_some() {
                    let mut kept = Vec::new();
                    for f in files {
                        if note_matches(
                            &self.store,
                            &self.email,
                            &p.slug,
                            &f,
                            p.r#type.as_deref(),
                            p.tags.as_deref(),
                        )
                        .await
                        {
                            kept.push(f);
                        }
                    }
                    kept
                } else {
                    files
                };
                self.log_access(&p.slug, p.path.as_deref().unwrap_or("-"));
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    serde_json::to_string(&files).unwrap_or_else(|_| "[]".into()),
                )]))
            }
            Err(e) => tool_error(e.mcp_message()),
        }
    }

    /// Read one markdown note.
    #[tool(description = "Read a markdown note from a vault")]
    pub async fn read_note(
        &self,
        Parameters(p): Parameters<SlugPath>,
    ) -> Result<CallToolResult, ErrorData> {
        // Thin wrapper: the helper returns raw bytes (REST needs them for
        // ETags); non-UTF-8 maps to the legacy read-failure string.
        match crate::api::read_note(&self.store, &self.email, &p.slug, &p.path).await {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(content) => {
                    self.log_access(&p.slug, &p.path);
                    Ok(CallToolResult::success(vec![ContentBlock::text(content)]))
                }
                Err(_) => tool_error(crate::api::ApiReadError::Unreadable.mcp_message()),
            },
            Err(e) => tool_error(e.mcp_message()),
        }
    }

    /// Search markdown content across accessible vaults.
    #[tool(description = "Search markdown content across the accessible vaults")]
    pub async fn search_notes(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Thin wrapper: ACL + cap live in crate::api (D-04, D-10).
        match crate::api::search_notes(&self.store, &self.email, &p.query, p.slug.as_deref()).await
        {
            Ok(hits) => {
                // Post-ACL filter (D-08): hits already come from accessible
                // vaults only; each hit carries its own slug for the check.
                let hits = if p.r#type.is_some() || p.tags.is_some() {
                    let mut kept = Vec::new();
                    for h in hits {
                        if note_matches(
                            &self.store,
                            &self.email,
                            &h.slug,
                            &h.path,
                            p.r#type.as_deref(),
                            p.tags.as_deref(),
                        )
                        .await
                        {
                            kept.push(h);
                        }
                    }
                    kept
                } else {
                    hits
                };
                // Same logging as before: one row per searched vault.
                match p.slug.as_deref() {
                    Some(slug) => self.log_access(slug, "-"),
                    None => {
                        for a in
                            crate::api::accessible_vault_entries(&self.store, &self.email).await
                        {
                            if crate::webdav::source_dir_of(Path::new(&a.vault.book_path)).is_some()
                            {
                                self.log_access(&a.vault.slug, "-");
                            }
                        }
                    }
                }
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    serde_json::to_string(&hits).unwrap_or_else(|_| "[]".into()),
                )]))
            }
            Err(e) => tool_error(e.mcp_message()),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MoriMcp {}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> Store {
        Store::connect_memory().await.expect("memory store")
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("morinet-mcp-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn safe_join_refuses_traversal_and_absolute_paths() {
        let src = Path::new("/srv/vault");
        assert_eq!(
            safe_join(src, "a/b.md").unwrap(),
            PathBuf::from("/srv/vault/a/b.md")
        );
        assert!(safe_join(src, "../escape.md").is_none());
        assert!(safe_join(src, "a/../../escape.md").is_none());
        assert!(safe_join(src, "/etc/passwd").is_none());
    }

    #[test]
    fn list_md_files_sorts_and_skips_hidden_dirs() {
        let d = tmpdir("list");
        std::fs::create_dir_all(d.join("concepts")).unwrap();
        std::fs::create_dir_all(d.join(".morinet-build/book")).unwrap();
        std::fs::write(d.join("index.md"), "# Home").unwrap();
        std::fs::write(d.join("concepts/z.md"), "Z").unwrap();
        std::fs::write(d.join(".morinet-build/book/index.html"), "x").unwrap();
        let files = list_md_files(&d);
        assert_eq!(
            files,
            vec!["concepts/z.md".to_string(), "index.md".to_string()]
        );
    }

    #[test]
    fn search_is_case_insensitive_and_capped() {
        let d = tmpdir("search");
        std::fs::write(d.join("a.md"), "alpha\nThe Quick Brown fox\nomega").unwrap();
        std::fs::write(d.join("b.md"), "nothing here").unwrap();
        let hits = search_md(&[("v".into(), d.clone())], "quick brown", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].path.as_str(), hits[0].line), ("a.md", 2));
        let capped = search_md(&[("v".into(), d.clone())], "the", 1);
        assert_eq!(capped.len(), 1, "cap respected");
    }

    #[tokio::test]
    async fn tools_are_acl_scoped() {
        // A reader without any grant sees nothing and gets a tool error on
        // the guessed slug — no existence leak.
        let s = store().await;
        s.create_user("reader@mcp.test", "pw", "reader")
            .await
            .unwrap();
        s.create_user("admin@mcp.test", "pw", "admin")
            .await
            .unwrap();
        let m = MoriMcp::new(s.clone(), "reader@mcp.test".into());

        let res = m.list_vaults().await.unwrap();
        let body = match &res.content[0] {
            ContentBlock::Text(t) => t.text.clone(),
            other => panic!("expected text block, got {other:?}"),
        };
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            json.as_array().unwrap().len(),
            0,
            "no vaults for a reader with no grants"
        );

        let res = m
            .list_notes(Parameters(ListNotesParams {
                slug: "secret".into(),
                path: None,
                r#type: None,
                tags: None,
            }))
            .await
            .unwrap();
        assert!(res.is_error == Some(true), "unknown slug is a tool error");
    }

    #[tokio::test]
    async fn read_note_refuses_traversal_even_with_access() {
        let s = store().await;
        s.ensure_admin("admin@mcp.test", "pw").await.unwrap();
        let src = tmpdir("read");
        std::fs::write(src.join("note.md"), "# Note\ncontent").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        std::fs::write(src.join("book/index.html"), "<html>").unwrap();
        s.seed_vault("rd", "Read vault", &src.join("book"))
            .await
            .unwrap();

        let m = MoriMcp::new(s.clone(), "admin@mcp.test".into());
        let ok = m
            .read_note(Parameters(SlugPath {
                slug: "rd".into(),
                path: "note.md".into(),
            }))
            .await
            .unwrap();
        assert_ne!(ok.is_error, Some(true), "reading a real note works: {ok:?}");

        let bad = m
            .read_note(Parameters(SlugPath {
                slug: "rd".into(),
                path: "../secret.md".into(),
            }))
            .await
            .unwrap();
        assert_eq!(bad.is_error, Some(true), "traversal refused");
    }

    // ---- access logging (plan 10.1-03, REQ-VLT-011) ----

    fn text(res: &CallToolResult) -> String {
        match &res.content[0] {
            ContentBlock::Text(t) => t.text.clone(),
            other => panic!("expected text block, got {other:?}"),
        }
    }

    /// Poll briefly for a specific spawned best-effort row to land.
    async fn poll_log(s: &Store, email: &str, want: (&str, &str, &str)) -> bool {
        for _ in 0..40 {
            let hit: Option<(i64,)> = sqlx::query_as(
                "SELECT id FROM access_log WHERE email = ? AND vault_slug = ? AND source = ? AND path = ? LIMIT 1",
            )
            .bind(email)
            .bind(want.0)
            .bind(want.1)
            .bind(want.2)
            .fetch_optional(&s.pool)
            .await
            .unwrap();
            if hit.is_some() {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        false
    }

    #[tokio::test]
    async fn mcp_read_note_logs_access() {
        let s = store().await;
        s.ensure_admin("log@mcp.test", "pw").await.unwrap();
        let src = tmpdir("log-read");
        std::fs::write(src.join("note.md"), "# Note\ncontent").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault("lr", "Log read", &src.join("book"))
            .await
            .unwrap();

        let m = MoriMcp::new(s.clone(), "log@mcp.test".into());
        let ok = m
            .read_note(Parameters(SlugPath {
                slug: "lr".into(),
                path: "note.md".into(),
            }))
            .await
            .unwrap();
        assert_ne!(ok.is_error, Some(true));

        let rows = poll_log(&s, "log@mcp.test", ("lr", "mcp", "note.md")).await;
        assert!(rows, "expected (lr, mcp, note.md) row for log@mcp.test");
    }

    #[tokio::test]
    async fn mcp_list_and_search_log() {
        let s = store().await;
        s.ensure_admin("log2@mcp.test", "pw").await.unwrap();
        let src = tmpdir("log-list");
        std::fs::write(src.join("note.md"), "needle here").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault("ll", "Log list", &src.join("book"))
            .await
            .unwrap();

        let m = MoriMcp::new(s.clone(), "log2@mcp.test".into());
        let res = m.list_vaults().await.unwrap();
        assert_ne!(res.is_error, Some(true));
        let rows = poll_log(&s, "log2@mcp.test", ("-", "mcp", "-")).await;
        assert!(rows, "list_vaults logs slug='-' path='-'");

        m.search_notes(Parameters(SearchParams {
            query: "needle".into(),
            slug: Some("ll".into()),
            r#type: None,
            tags: None,
        }))
        .await
        .unwrap();
        let rows = poll_log(&s, "log2@mcp.test", ("ll", "mcp", "-")).await;
        assert!(rows, "slug-scoped search logs one row for that slug");
    }

    #[tokio::test]
    async fn mcp_denied_call_logs_nothing() {
        let s = store().await;
        s.create_user("nobody@mcp.test", "pw", "reader")
            .await
            .unwrap();
        let m = MoriMcp::new(s.clone(), "nobody@mcp.test".into());
        let res = m
            .read_note(Parameters(SlugPath {
                slug: "ghost".into(),
                path: "x.md".into(),
            }))
            .await
            .unwrap();
        assert_eq!(res.is_error, Some(true));
        // Give any (wrongly) spawned insert a chance to land before asserting.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let rows = sqlx::query_as::<_, (String, String, String)>(
            "SELECT vault_slug, source, path FROM access_log WHERE email = ?",
        )
        .bind("nobody@mcp.test")
        .fetch_all(&s.pool)
        .await
        .unwrap();
        assert!(rows.is_empty(), "denied calls log nothing: {rows:?}");
    }

    // ---- share ACL through MCP tokens (plan 10.1-03, REQ-VLT-010) ----

    /// Owner vault + sharee user; returns (store, sharee MoriMcp).
    async fn share_fixture(tag: &str, slug: &str, role: &str) -> (Store, MoriMcp) {
        let s = store().await;
        s.create_user("owner@share.test", "pw", "reader")
            .await
            .unwrap();
        s.create_user("sharee@share.test", "pw", "reader")
            .await
            .unwrap();
        let src = tmpdir(tag);
        std::fs::write(src.join("note.md"), "# Shared\nsecret needle content").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault(slug, "Shared vault", &src.join("book"))
            .await
            .unwrap();
        s.share_vault(slug, "sharee@share.test", role)
            .await
            .unwrap();
        let m = MoriMcp::new(s.clone(), "sharee@share.test".into());
        (s, m)
    }

    #[tokio::test]
    async fn mcp_sharee_sees_shared_vault() {
        // reader share and writer share both list + read via MCP (MCP is
        // read-only; writer's extra rights live web/WebDAV-side).
        for role in ["reader", "writer"] {
            let want_role = if role == "writer" { "write" } else { "read" };
            let (_s, m) = share_fixture(&format!("share-{role}"), "sh1", role).await;
            let res = m.list_vaults().await.unwrap();
            let body = text(&res);
            assert!(
                body.contains("\"sh1\"") && body.contains(&format!("\"{want_role}\"")),
                "{role} share listed with role {want_role}: {body}"
            );

            let ok = m
                .read_note(Parameters(SlugPath {
                    slug: "sh1".into(),
                    path: "note.md".into(),
                }))
                .await
                .unwrap();
            assert_ne!(ok.is_error, Some(true), "{role} can read: {ok:?}");
            assert!(text(&ok).contains("secret needle content"));

            let listed = m
                .list_notes(Parameters(ListNotesParams {
                    slug: "sh1".into(),
                    path: None,
                    r#type: None,
                    tags: None,
                }))
                .await
                .unwrap();
            assert_ne!(listed.is_error, Some(true), "{role} can list notes");
        }
    }

    #[tokio::test]
    async fn mcp_hidden_share_invisible() {
        let (s, m) = share_fixture("share-hidden", "sh2", "reader").await;
        s.hide_share("sh2", "sharee@share.test").await.unwrap();

        let res = m.list_vaults().await.unwrap();
        assert!(
            !text(&res).contains("sh2"),
            "hidden share absent from list_vaults"
        );
        let bad = m
            .read_note(Parameters(SlugPath {
                slug: "sh2".into(),
                path: "note.md".into(),
            }))
            .await
            .unwrap();
        assert_eq!(bad.is_error, Some(true));
        assert_eq!(text(&bad), "vault not found or access denied");
    }

    #[tokio::test]
    async fn mcp_revoked_share_denied() {
        let (s, m) = share_fixture("share-revoked", "sh3", "reader").await;
        // Access works before revocation, is gone immediately after.
        let ok = m
            .read_note(Parameters(SlugPath {
                slug: "sh3".into(),
                path: "note.md".into(),
            }))
            .await
            .unwrap();
        assert_ne!(ok.is_error, Some(true));

        s.revoke_share("sh3", "sharee@share.test").await.unwrap();
        let bad = m
            .read_note(Parameters(SlugPath {
                slug: "sh3".into(),
                path: "note.md".into(),
            }))
            .await
            .unwrap();
        assert_eq!(bad.is_error, Some(true));
        assert_eq!(text(&bad), "vault not found or access denied");
        assert!(!text(&m.list_vaults().await.unwrap()).contains("sh3"));
    }

    #[tokio::test]
    async fn mcp_search_finds_notes_in_shared_vault() {
        let (_s, m) = share_fixture("share-search", "sh4", "reader").await;
        let res = m
            .search_notes(Parameters(SearchParams {
                query: "secret needle".into(),
                slug: None,
                r#type: None,
                tags: None,
            }))
            .await
            .unwrap();
        assert_ne!(res.is_error, Some(true));
        let body = text(&res);
        assert!(
            body.contains("\"sh4\""),
            "global search hits the shared vault: {body}"
        );
    }

    // ---- type/tags filters, applied strictly post-ACL (plan 18-06, D-08) ----

    /// Vault with three notes: `concept.md` (`type: concept`,
    /// `tags: [music, history]`), `entity.md` (`type: entity`,
    /// `tags: [music]`) and `plain.md` (no frontmatter). All three carry the
    /// body text `shared needle` so a search query hits every one of them.
    /// Returns the store; the caller's user is an admin (full access).
    async fn filter_fixture(tag: &str, slug: &str, email: &str) -> Store {
        let s = store().await;
        s.ensure_admin(email, "pw").await.unwrap();
        let src = tmpdir(tag);
        std::fs::write(
            src.join("concept.md"),
            "---\ntype: concept\ntags: [music, history]\n---\nshared needle text\n",
        )
        .unwrap();
        std::fs::write(
            src.join("entity.md"),
            "---\ntype: entity\ntags: [music]\n---\nshared needle text\n",
        )
        .unwrap();
        std::fs::write(src.join("plain.md"), "shared needle text\n").unwrap();
        std::fs::create_dir_all(src.join("book")).unwrap();
        s.seed_vault(slug, "Filter vault", &src.join("book"))
            .await
            .unwrap();
        s
    }

    /// Run `list_notes` and parse the JSON array of paths.
    async fn listed(m: &MoriMcp, p: ListNotesParams) -> Vec<String> {
        let res = m.list_notes(Parameters(p)).await.unwrap();
        assert_ne!(res.is_error, Some(true), "expected success: {res:?}");
        serde_json::from_str(&text(&res)).unwrap()
    }

    #[tokio::test]
    async fn list_notes_filter_by_type() {
        let s = filter_fixture("okf-type", "ft", "flt1@mcp.test").await;
        let m = MoriMcp::new(s, "flt1@mcp.test".into());
        let got = listed(
            &m,
            ListNotesParams {
                slug: "ft".into(),
                path: None,
                r#type: Some("concept".into()),
                tags: None,
            },
        )
        .await;
        assert_eq!(got, vec!["concept.md".to_string()]);
    }

    #[tokio::test]
    async fn list_notes_filter_by_tags_any() {
        let s = filter_fixture("okf-tags", "tg", "flt2@mcp.test").await;
        let m = MoriMcp::new(s, "flt2@mcp.test".into());
        let got = listed(
            &m,
            ListNotesParams {
                slug: "tg".into(),
                path: None,
                r#type: None,
                tags: Some(vec!["history".into()]),
            },
        )
        .await;
        assert_eq!(got, vec!["concept.md".to_string()], "ANY match");

        let got = listed(
            &m,
            ListNotesParams {
                slug: "tg".into(),
                path: None,
                r#type: None,
                tags: Some(vec!["music".into()]),
            },
        )
        .await;
        assert_eq!(
            got,
            vec!["concept.md".to_string(), "entity.md".to_string()],
            "music is on both frontmatter notes; plain.md has none"
        );
    }

    #[tokio::test]
    async fn list_notes_filter_type_and_tags_and() {
        let s = filter_fixture("okf-and", "an", "flt3@mcp.test").await;
        let m = MoriMcp::new(s, "flt3@mcp.test".into());
        let got = listed(
            &m,
            ListNotesParams {
                slug: "an".into(),
                path: None,
                r#type: Some("entity".into()),
                tags: Some(vec!["history".into()]),
            },
        )
        .await;
        assert!(got.is_empty(), "AND combination ⇒ empty, not error");
    }

    #[tokio::test]
    async fn list_notes_no_filters_unchanged() {
        // Regression: omitted filters ⇒ byte-identical to pre-filter behavior.
        let s = filter_fixture("okf-none", "nn", "flt4@mcp.test").await;
        let m = MoriMcp::new(s, "flt4@mcp.test".into());
        let got = listed(
            &m,
            ListNotesParams {
                slug: "nn".into(),
                path: None,
                r#type: None,
                tags: None,
            },
        )
        .await;
        assert_eq!(
            got,
            vec![
                "concept.md".to_string(),
                "entity.md".to_string(),
                "plain.md".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn search_notes_filter_by_type() {
        let s = filter_fixture("okf-search", "sr", "flt5@mcp.test").await;
        let m = MoriMcp::new(s, "flt5@mcp.test".into());
        let res = m
            .search_notes(Parameters(SearchParams {
                query: "shared needle".into(),
                slug: Some("sr".into()),
                r#type: Some("entity".into()),
                tags: None,
            }))
            .await
            .unwrap();
        assert_ne!(res.is_error, Some(true), "{res:?}");
        let hits: Vec<serde_json::Value> = serde_json::from_str(&text(&res)).unwrap();
        assert_eq!(hits.len(), 1, "only entity.md survives: {hits:?}");
        assert_eq!(hits[0]["path"], "entity.md");
    }

    #[tokio::test]
    async fn mcp_filters_never_resurrect_inaccessible_vault() {
        // T-18-15: a filter must not become an oracle on private vaults —
        // the ACL error is identical with and without filters (D-08).
        let s = filter_fixture("okf-acl", "fr", "owner-flt@mcp.test").await;
        s.create_user("bob@mcp.test", "pw", "reader").await.unwrap();
        let bob = MoriMcp::new(s, "bob@mcp.test".into());

        let plain = bob
            .list_notes(Parameters(ListNotesParams {
                slug: "fr".into(),
                path: None,
                r#type: None,
                tags: None,
            }))
            .await
            .unwrap();
        let filtered = bob
            .list_notes(Parameters(ListNotesParams {
                slug: "fr".into(),
                path: None,
                r#type: Some("concept".into()),
                tags: Some(vec!["music".into()]),
            }))
            .await
            .unwrap();
        assert_eq!(plain.is_error, Some(true));
        assert_eq!(filtered.is_error, Some(true), "filter ⇒ still an error");
        assert_eq!(
            text(&plain),
            text(&filtered),
            "same tool_error message — no existence oracle"
        );

        let s_plain = bob
            .search_notes(Parameters(SearchParams {
                query: "shared needle".into(),
                slug: Some("fr".into()),
                r#type: None,
                tags: None,
            }))
            .await
            .unwrap();
        let s_filtered = bob
            .search_notes(Parameters(SearchParams {
                query: "shared needle".into(),
                slug: Some("fr".into()),
                r#type: Some("concept".into()),
                tags: Some(vec!["music".into()]),
            }))
            .await
            .unwrap();
        assert_eq!(s_plain.is_error, Some(true));
        assert_eq!(s_filtered.is_error, Some(true));
        assert_eq!(text(&s_plain), text(&s_filtered));
    }

    #[tokio::test]
    async fn mcp_filter_params_schema_exposes_type_and_tags() {
        // The schemars path (rmcp #[tool]) must publish the new optional
        // params so clients can discover them.
        use schemars::schema_for;
        for schema in [schema_for!(ListNotesParams), schema_for!(SearchParams)] {
            let v = serde_json::to_value(&schema).unwrap();
            let props = v["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("no properties: {v}"));
            let ty = props
                .get("type")
                .unwrap_or_else(|| panic!("no `type` property: {v}"));
            let tg = props
                .get("tags")
                .unwrap_or_else(|| panic!("no `tags` property: {v}"));
            assert!(
                serde_json::to_string(ty).unwrap().contains("string"),
                "`type` is a string param: {ty}"
            );
            assert!(
                serde_json::to_string(tg).unwrap().contains("array"),
                "`tags` is an array param: {tg}"
            );
        }
    }
}
