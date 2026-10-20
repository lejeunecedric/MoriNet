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

    /// Run `mdbook build` on the synced directory when mdbook is available.
    /// Returns the compiled book directory.
    ///
    /// - `book.toml` present → build directly (book dir = `<src>/book`)
    /// - no `book.toml` but plain markdown wiki (index.md or ≥3 .md files) →
    ///   auto-scaffold a book (`.morinet-book/`) then build it (B3Wiki/ProgWiki style)
    /// - otherwise (no markdown at all) → serve the dir as static content
    fn build(&self, src_dir: &Path) -> anyhow::Result<PathBuf> {
        let book = src_dir.join("book");
        if src_dir.join("book.toml").is_file() {
            return self.run_mdbook_build(src_dir, &book);
        }

        match count_markdown_files(src_dir) {
            n if n >= 3 || (n > 0 && src_dir.join("index.md").is_file()) => {
                let scaffold = self.scaffold_plain_wiki(src_dir)?;
                let book = scaffold.join("book");
                self.run_mdbook_build(&scaffold, &book)
            }
            _ => {
                // No markdown worth a book — serve the dir as-is (static).
                Ok(src_dir.to_path_buf())
            }
        }
    }

    /// Generate a throwaway mdbook scaffold (`.morinet-book/`) for a plain wiki:
    /// SUMMARY.md grouped by top-level directory, content copied preserving
    /// relative paths (assets included). Regenerated from scratch every sync.
    fn scaffold_plain_wiki(&self, src: &Path) -> anyhow::Result<PathBuf> {
        use std::collections::BTreeMap;

        let scaffold = src.join(".morinet-book");
        if scaffold.exists() {
            std::fs::remove_dir_all(&scaffold)?;
        }
        let content = scaffold.join("src");
        std::fs::create_dir_all(&content)?;

        // Collect markdown files + other files (assets), skipping .git/scaffold.
        fn walk(
            dir: &Path,
            mds: &mut Vec<std::path::PathBuf>,
            others: &mut Vec<std::path::PathBuf>,
        ) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name == ".git" || name == ".morinet-book" {
                    continue;
                }
                if path.is_dir() {
                    walk(&path, mds, others);
                } else if path.extension().map(|e| e == "md").unwrap_or(false) {
                    mds.push(path);
                } else {
                    others.push(path);
                }
            }
        }
        let mut mds = Vec::new();
        let mut others = Vec::new();
        walk(src, &mut mds, &mut others);

        // index.md first, then alphabetical.
        mds.sort();
        if let Some(pos) = mds
            .iter()
            .position(|p| p.file_name().map(|n| n == "index.md").unwrap_or(false))
        {
            let idx = mds.remove(pos);
            mds.insert(0, idx);
        }

        // Group by top-level directory for a readable SUMMARY.
        let mut groups: BTreeMap<String, Vec<(String, std::path::PathBuf)>> = BTreeMap::new();
        for md in &mds {
            let rel = md.strip_prefix(src)?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let in_subdir = md.parent().map(|p| p != src).unwrap_or(false);
            let group = if in_subdir {
                rel.components()
                    .next()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .unwrap_or_else(|| "Pages".to_string())
            } else {
                "Home".to_string()
            };
            let label = prettify_label(rel);
            groups.entry(group).or_default().push((rel_str, md.clone()));
        }

        let mut summary = String::from("# Summary\n\n");
        let mut write_group = |name: &str,
                               items: Vec<(String, std::path::PathBuf)>,
                               summary: &mut String,
                               content: &Path|
         -> anyhow::Result<()> {
            if !summary.ends_with("\n\n") {
                summary.push('\n');
            }
            summary.push_str(&format!("# {}\n\n", prettify_dir(name)));
            for (rel_str, md) in items {
                let dest = content.join(&rel_str);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(md, &dest)?;
                summary.push_str(&format!(
                    "- [{label}]({rel_str})\n",
                    label = label_of(&rel_str)
                ));
            }
            Ok(())
        };

        // "Home" group first, then the rest alphabetically.
        if let Some(home) = groups.remove("Home") {
            write_group("Home", home, &mut summary, &content)?;
        }
        for (name, items) in groups {
            write_group(&name, items, &mut summary, &content)?;
        }

        // Copy non-markdown files (assets, images) preserving structure.
        for f in &others {
            let rel = f.strip_prefix(src)?;
            let dest = content.join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(f, &dest)?;
        }

        let title = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Wiki".to_string());
        std::fs::write(
            scaffold.join("book.toml"),
            format!("[book]\ntitle = \"{title}\"\nauthors = []\n[build]\nbuild-dir = \"book\"\n"),
        )?;
        std::fs::write(scaffold.join("src").join("SUMMARY.md"), summary)?;

        Ok(scaffold)
    }

    fn run_mdbook_build(&self, src_dir: &Path, book: &Path) -> anyhow::Result<PathBuf> {
        if book.join("index.html").is_file() {
            return Ok(book.to_path_buf());
        }
        match std::process::Command::new("mdbook")
            .arg("build")
            .arg(src_dir)
            .output()
        {
            Ok(out) if out.status.success() => Ok(book.to_path_buf()),
            Ok(out) => anyhow::bail!(
                "mdbook build failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "warning: `mdbook` not found in PATH; serving last compiled book at {}",
                    book.display()
                );
                Ok(book.to_path_buf())
            }
            Err(e) => anyhow::bail!("failed to run mdbook: {e}"),
        }
    }
}

/// Count `.md` files recursively (skipping `.git` and the scaffold dir).
/// Free function (not a trait method) so `VaultSource` stays dyn-compatible.
fn count_markdown_files(dir: &Path) -> usize {
    fn walk(dir: &Path, count: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == ".git" || name == ".morinet-book" {
                continue;
            }
            if path.is_dir() {
                walk(&path, count);
            } else if path.extension().map(|e| e == "md").unwrap_or(false) {
                *count += 1;
            }
        }
    }
    let mut count = 0;
    walk(dir, &mut count);
    count
}

fn prettify_dir(name: &str) -> String {
    name.replace(['-', '_'], " ")
}

fn prettify_label(rel: &Path) -> String {
    let stem = rel
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "page".to_string());
    let mut label = stem.replace(['-', '_'], " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

fn label_of(rel_str: &str) -> String {
    let stem = rel_str
        .rsplit('/')
        .next()
        .unwrap_or(rel_str)
        .trim_end_matches(".md");
    let mut label = stem.replace(['-', '_'], " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
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
    fn build_skips_dirs_without_book_toml() {
        let dir = tmp("build-plain");
        let src = LocalSource { path: dir.clone() };
        let out = src.build(&dir).unwrap();
        assert_eq!(out, dir, "no book.toml → serve as-is");
    }

    #[test]
    fn build_returns_book_dir_when_compiled() {
        let dir = tmp("build-compiled");
        std::fs::write(dir.join("book.toml"), "[book]\n").unwrap();
        std::fs::create_dir_all(dir.join("book")).unwrap();
        std::fs::write(dir.join("book").join("index.html"), "<html></html>").unwrap();
        let src = LocalSource { path: dir.clone() };
        assert_eq!(src.build(&dir).unwrap(), dir.join("book"));
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
