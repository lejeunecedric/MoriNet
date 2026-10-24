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
            Ok(content) => Ok(CallToolResult::success(vec![ContentBlock::text(content)])),
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
}
