//! Vault source plugins (AD-8): local folders or GitHub repos (clone + sync).
//!
//! A `VaultSource` resolves to a local book-source directory. After a successful
//! sync we run `mdbook build` (T3.3) and register the resulting `book/` dir.

use std::path::{Path, PathBuf};

use git2::{Repository, ResetType};

/// A source that can materialize a book-source directory locally (idempotent).
pub trait VaultSource: Send + Sync {
    /// Human-readable kind, for logs ("local", "github").
    fn kind(&self) -> &'static str;

    /// Bring the local copy up to date and return the book-source directory.
    fn sync(&self) -> anyhow::Result<PathBuf>;

    /// Bring the vault to a servable compiled book (Phase 8b, decisions
    /// D2/AD-14 — dispatch on `obsidian::detect`):
    /// - `Native` (book.toml or src/SUMMARY.md): `mdbook build <src>` → `<src>/book`.
    /// - `LlmWiki` (llm-wiki/Obsidian vault): convert into `<src>/.morinet-build/`
    ///   first, then build that tree → `<src>/.morinet-build/book`. Always
    ///   re-converted — links, SUMMARY and meta must reflect the latest sync.
    /// - `Neither` (matching neither format): registered as-is; the caller is
    ///   responsible for surfacing the admin warning (PRD-6).
    fn build(&self, src_dir: &Path, vault_name: &str) -> anyhow::Result<PathBuf> {
        use crate::obsidian::convert::{convert, detect, VaultFormat};
        let format = detect(src_dir);
        let build_dir = match format {
            VaultFormat::LlmWiki => convert(src_dir, vault_name)?.build_dir,
            VaultFormat::Native => src_dir.to_path_buf(),
            VaultFormat::Neither => return Ok(src_dir.to_path_buf()),
        };
        let book = build_dir.join("book");
        // Native books keep the legacy skip-when-compiled fast path; llm-wiki
        // books are rebuilt on every sync (freshness is the point, D5).
        if matches!(format, VaultFormat::Native) && book.join("index.html").is_file() {
            return Ok(book);
        }
        let build_result: anyhow::Result<()> = match std::process::Command::new("mdbook")
            .arg("build")
            .arg(&build_dir)
            .output()
        {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => anyhow::bail!(
                "mdbook build failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => anyhow::bail!(
                "`mdbook` not found in PATH — install it (and in the morinet service PATH) to compile vaults"
            ),
            Err(e) => anyhow::bail!("failed to run mdbook: {e}"),
        };
        build_result?;
        // A book that never compiled must fail LOUDLY here: a silent fallback
        // surfaces later as an undebuggable 404 on /vault/<slug>/.
        if !book.join("index.html").is_file() {
            anyhow::bail!(
                "book not compiled at {} — `mdbook` ran but produced no index.html",
                book.display()
            );
        }
        Ok(book)
    }
}

/// A local, already-present book directory (or mdbook source dir).
pub struct LocalSource {
    pub path: PathBuf,
}

impl VaultSource for LocalSource {
    fn kind(&self) -> &'static str {
        "local"
    }

    fn sync(&self) -> anyhow::Result<PathBuf> {
        if !self.path.is_dir() {
            anyhow::bail!("local vault path does not exist: {}", self.path.display());
        }
        Ok(self.path.clone())
    }
}

/// A GitHub (or any git) repository, cloned/pulled into `dest`.
///
/// Private repos: pass a token; it is injected into the HTTPS URL
/// (`https://x-access-token:<token>@host/...`) — documented v1 tradeoff.
pub struct GitSource {
    pub url: String,
    pub token: Option<String>,
    pub dest: PathBuf,
}

impl GitSource {
    pub fn github(url: impl Into<String>, token: Option<String>, dest: PathBuf) -> Self {
        Self {
            url: url.into(),
            token,
            dest,
        }
    }

    fn effective_url(&self) -> String {
        match &self.token {
            Some(token) if !token.is_empty() => {
                // https://github.com/owner/repo -> https://x-access-token:TOKEN@github.com/owner/repo
                self.url
                    .replacen("https://", &format!("https://x-access-token:{token}@"), 1)
            }
            _ => self.url.clone(),
        }
    }

    fn clone_new(&self) -> anyhow::Result<PathBuf> {
        if let Some(parent) = self.dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Repository::clone(&self.effective_url(), &self.dest)?;
        Ok(self.dest.clone())
    }

    fn fetch_and_reset(&self) -> anyhow::Result<PathBuf> {
        let repo = Repository::open(&self.dest)?;
        let mut remote = repo.find_remote("origin")?;
        remote.fetch(&["+refs/heads/*:refs/remotes/origin/*"], None, None)?;
        let fetch_head = repo.find_reference("FETCH_HEAD")?;
        let annotated = repo.reference_to_annotated_commit(&fetch_head)?;
        let obj = repo.find_object(annotated.id(), Some(git2::ObjectType::Commit))?;
        repo.reset(&obj, ResetType::Hard, None)?;
        Ok(self.dest.clone())
    }
}

impl VaultSource for GitSource {
    fn kind(&self) -> &'static str {
        "github"
    }

    fn sync(&self) -> anyhow::Result<PathBuf> {
        if self.dest.join(".git").is_dir() {
            self.fetch_and_reset()
        } else {
            self.clone_new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("morinet-src-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Create an origin repo with one commit on the default branch.
    fn init_origin(dir: &Path, content: &str) {
        let repo = Repository::init(dir).unwrap();
        std::fs::write(dir.join("book.toml"), "[book]\ntitle = \"t\"\n").unwrap();
        std::fs::write(dir.join("page.html"), content).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("book.toml")).unwrap();
        index.add_path(Path::new("page.html")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("test", "test@test").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        drop(tree); // release borrow before `repo` moves out
    }

    fn commit_file(repo: &Repository, dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("test", "test@test").unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "update", &tree, &[&parent])
            .unwrap();
    }

    #[test]
    fn local_source_returns_path_and_validates() {
        let dir = tmp("local-ok");
        let src = LocalSource { path: dir.clone() };
        assert_eq!(src.kind(), "local");
        assert_eq!(src.sync().unwrap(), dir);

        let missing = LocalSource {
            path: dir.join("nope"),
        };
        assert!(missing.sync().is_err(), "missing path rejected");
    }

    #[test]
    fn build_serves_neither_format_dir_as_is() {
        let dir = tmp("build-plain");
        let src = LocalSource { path: dir.clone() };
        let out = src.build(&dir, "Plain").unwrap();
        assert_eq!(out, dir, "matches no format → serve as-is (admin warns)");
    }

    #[test]
    fn build_returns_book_dir_when_compiled() {
        let dir = tmp("build-compiled");
        std::fs::write(dir.join("book.toml"), "[book]\n").unwrap();
        std::fs::create_dir_all(dir.join("book")).unwrap();
        std::fs::write(dir.join("book").join("index.html"), "<html></html>").unwrap();
        let src = LocalSource { path: dir.clone() };
        assert_eq!(src.build(&dir, "T").unwrap(), dir.join("book"));
    }

    #[test]
    fn build_converts_llm_wiki_before_building() {
        // The wiring that makes phase 8b real: a wiki dir (no book.toml) goes
        // through convert(), NOT the legacy static fallback. Asserted on the
        // conversion artifacts so it passes whether or not mdbook is installed.
        let dir = tmp("build-llmwiki");
        std::fs::write(
            dir.join("index.md"),
            "---\ntitle: Home\n---\n\n# Home\n\n[[x]]\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("concepts")).unwrap();
        std::fs::write(
            dir.join("concepts/x.md"),
            "---\ntitle: X\ntype: concept\n---\n\nConcept.\n",
        )
        .unwrap();
        let src = LocalSource { path: dir.clone() };
        let book = src.build(&dir, "My Wiki").unwrap();
        assert_eq!(book, dir.join(".morinet-build").join("book"));
        let build = dir.join(".morinet-build");
        assert!(build.join("book.toml").is_file(), "convert ran");
        let summary = std::fs::read_to_string(build.join("src/SUMMARY.md")).unwrap();
        assert!(summary.contains("[Home](index.md)"));
        let x = std::fs::read_to_string(build.join("src/concepts/x.md")).unwrap();
        assert!(x.contains("note-type"), "meta block applied");
    }

    #[test]
    fn git_source_clones_then_pulls_new_commits() {
        let origin_dir = tmp("git-origin");
        init_origin(&origin_dir, "v1 content");

        let dest = tmp("git-clone");
        let src = GitSource::github(
            format!("file://{}", origin_dir.display()),
            None,
            dest.clone(),
        );
        assert_eq!(src.kind(), "github");

        // First sync = clone.
        let synced = src.sync().expect("clone");
        assert_eq!(synced, dest);
        assert!(dest.join(".git").is_dir());
        assert_eq!(
            std::fs::read_to_string(dest.join("page.html")).unwrap(),
            "v1 content"
        );

        // Upstream moves: new commit on the origin repo.
        let repo = Repository::open(&origin_dir).unwrap();
        commit_file(&repo, &origin_dir, "page.html", "v2 content");

        // Second sync = fetch + hard reset to origin head.
        src.sync().expect("pull");
        assert_eq!(
            std::fs::read_to_string(dest.join("page.html")).unwrap(),
            "v2 content",
            "second sync picked up the new commit"
        );
    }

    #[test]
    fn git_source_token_is_injected_into_url() {
        let src = GitSource::github(
            "https://github.com/owner/repo".to_string(),
            Some("tok123".to_string()),
            tmp("git-token"),
        );
        assert_eq!(
            src.effective_url(),
            "https://x-access-token:tok123@github.com/owner/repo"
        );
        let no_token = GitSource::github(
            "https://github.com/owner/repo".to_string(),
            None,
            tmp("git-notoken"),
        );
        assert_eq!(no_token.effective_url(), "https://github.com/owner/repo");
    }

    #[test]
    fn git_source_clone_of_missing_repo_fails() {
        let dest = tmp("git-bad");
        let src = GitSource::github("file:///nonexistent/repo.git".to_string(), None, dest);
        assert!(src.sync().is_err(), "clone of missing origin fails");
    }
}
