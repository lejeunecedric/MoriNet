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
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Case-insensitive substring to find in the markdown of accessible vaults.
    pub query: String,
    /// Restrict to one vault slug (defaults to every accessible vault).
    #[serde(default)]
    pub slug: Option<String>,
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

#[tool_router]
impl MoriMcp {
    pub fn new(store: Store, email: String) -> Self {
        Self {
            tool_router: Self::tool_router(),
            store,
            email,
        }
    }

    /// The source directory of an accessible vault (same resolution as
    /// WebDAV: the markdown source, never the generated tree). One generic
    /// error for unknown slug and no-access — no existence leak.
    async fn source_dir(&self, slug: &str) -> Option<PathBuf> {
        let entry = self
            .store
            .accessible_vaults(&self.email)
            .await
            .ok()?
            .into_iter()
            .find(|a| a.vault.slug == slug)?;
        crate::webdav::source_dir_of(Path::new(&entry.vault.book_path))
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
        let vaults: Vec<serde_json::Value> = self
            .store
            .accessible_vaults(&self.email)
            .await
            .unwrap_or_default()
            .iter()
            .map(|a| {
                serde_json::json!({
                    "slug": a.vault.slug,
                    "name": a.vault.name,
                    "scope": a.vault.scope,
                    "role": a.role,
                })
            })
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
        let Some(src) = self.source_dir(&p.slug).await else {
            return tool_error("vault not found or access denied");
        };
        let base = match p.path.as_deref() {
            Some(sub) => match safe_join(&src, sub) {
                Some(b) => b,
                None => return tool_error("invalid path"),
            },
            None => src.clone(),
        };
        if !base.is_dir() {
            return tool_error("no such folder");
        }
        let files = list_md_files(&base);
        self.log_access(&p.slug, p.path.as_deref().unwrap_or("-"));
        Ok(CallToolResult::success(vec![ContentBlock::text(
            serde_json::to_string(&files).unwrap_or_else(|_| "[]".into()),
        )]))
    }

    /// Read one markdown note.
    #[tool(description = "Read a markdown note from a vault")]
    pub async fn read_note(
        &self,
        Parameters(p): Parameters<SlugPath>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(src) = self.source_dir(&p.slug).await else {
            return tool_error("vault not found or access denied");
        };
        let full = match safe_join(&src, &p.path) {
            Some(f) => f,
            None => return tool_error("invalid path"),
        };
        if !full.is_file() {
            return tool_error("no such note");
        }
        match std::fs::read_to_string(&full) {
            Ok(content) => {
                self.log_access(&p.slug, &p.path);
                Ok(CallToolResult::success(vec![ContentBlock::text(content)]))
            }
            Err(_) => tool_error("could not read the note"),
        }
    }

    /// Search markdown content across accessible vaults.
    #[tool(description = "Search markdown content across the accessible vaults")]
    pub async fn search_notes(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut vaults: Vec<(String, PathBuf)> = Vec::new();
        if let Some(slug) = p.slug.as_deref() {
            match self.source_dir(slug).await {
                Some(dir) => vaults.push((slug.to_string(), dir)),
                None => return tool_error("vault not found or access denied"),
            }
        } else {
            for a in self
                .store
                .accessible_vaults(&self.email)
                .await
                .unwrap_or_default()
            {
                if let Some(dir) = crate::webdav::source_dir_of(Path::new(&a.vault.book_path)) {
                    vaults.push((a.vault.slug.clone(), dir));
                }
            }
        }
        let hits = search_md(&vaults, &p.query, 50);
        for (slug, _) in &vaults {
            self.log_access(slug, "-");
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(
            serde_json::to_string(&hits).unwrap_or_else(|_| "[]".into()),
        )]))
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
}
