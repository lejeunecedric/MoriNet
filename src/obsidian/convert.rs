//! Bite 4 of Phase 8b — whole-directory conversion: llm-wiki source →
//! generated mdBook tree in `<source>/.morinet-build/`.
//!
//! The source vault is NEVER modified (AD-14). The mirror holds every content
//! file (`.md` rewritten through frontmatter/links/tree, assets verbatim), so
//! relative links and embeds resolve after the move. Regenerated from scratch
//! at every sync — stale pages of deleted notes must not survive.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::{frontmatter, links, tree};

/// Format of a vault's contents, detected at sync (PRD-6, decision D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultFormat {
    /// mdBook-native: `book.toml` or `src/SUMMARY.md` present — served as-is.
    Native,
    /// llm-wiki / Obsidian: markdown tree without mdBook scaffolding.
    LlmWiki,
    /// Neither — the admin gets the warning (D2 wording).
    Neither,
}

/// Directories hidden from the conversion: Obsidian state, archives, our own
/// build output, node noise. Everything dot-prefixed is skipped anyway.
const EXCLUDED_DIRS: &[&str] = &["_archive", "node_modules"];

fn is_excluded_dir(name: &str) -> bool {
    name.starts_with('.') || EXCLUDED_DIRS.contains(&name)
}

/// Detect the vault format (D2): native scaffolding wins, then any `.md`
/// file means llm-wiki, else neither.
pub fn detect(source: &Path) -> VaultFormat {
    if source.join("book.toml").is_file() || source.join("src/SUMMARY.md").is_file() {
        return VaultFormat::Native;
    }
    if has_markdown(source) {
        return VaultFormat::LlmWiki;
    }
    VaultFormat::Neither
}

fn has_markdown(source: &Path) -> bool {
    let mut stack = vec![source.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !is_excluded_dir(&name) {
                    stack.push(path);
                }
            } else if name.ends_with(".md") {
                return true;
            }
        }
    }
    false
}

/// What a conversion produced (surfaced in logs / admin at bite 5).
#[derive(Debug)]
pub struct ConversionReport {
    pub build_dir: PathBuf,
    pub pages: usize,
    pub missing_links: usize,
}

/// Convert a whole llm-wiki directory into a buildable mdBook source tree.
pub fn convert(source: &Path, vault_name: &str) -> Result<ConversionReport> {
    use anyhow::Context;
    use std::fs;

    let build = source.join(".morinet-build");
    // Fresh tree every time: pages of deleted notes must not survive.
    if build.exists() {
        fs::remove_dir_all(&build).with_context(|| format!("clear {}", build.display()))?;
    }
    let src_root = build.join("src");
    fs::create_dir_all(&src_root)?;

    // Every mirrored file, vault-relative, posix separators.
    let mut files: Vec<String> = Vec::new();
    collect(source, source, &mut files).with_context(|| format!("scan {}", source.display()))?;
    let index = links::VaultIndex::build(files.clone());

    let mut entries: Vec<tree::PageEntry> = Vec::new();
    let mut missing_links = 0usize;
    for rel in &files {
        let from = source.join(rel);
        let to = src_root.join(rel);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        if rel.ends_with(".md") {
            let raw = fs::read_to_string(&from).with_context(|| format!("read {rel}"))?;
            let (fm, body) = frontmatter::split(&raw);
            let rewritten = links::rewrite(rel, body, &index);
            let mut page = tree::meta_block(&fm);
            page.push_str(&rewritten);
            missing_links += page.matches("wikilink-missing").count();
            fs::write(&to, page).with_context(|| format!("write {rel}"))?;
            entries.push(tree::PageEntry {
                rel: rel.clone(),
                title: tree::page_title(&fm, body, rel),
            });
        } else {
            fs::copy(&from, &to)?;
        }
    }

    // Landing pages for sub-folders without a native index: mdBook needs a
    // real hyperlink as the parent of nested chapters (measured with 0.5).
    add_folder_notes(&src_root, &files, &mut entries).context("generate folder notes")?;

    // Generated mdBook scaffolding.
    fs::write(src_root.join("SUMMARY.md"), tree::summary_md(&entries))?;
    fs::write(build.join("book.toml"), tree::book_toml(vault_name))?;
    let theme = build.join("theme");
    fs::create_dir_all(&theme)?;
    fs::write(theme.join("morinet.css"), tree::MORINET_CSS)?;
    // Zoom bar (ported from oscbrain) + the "close vault" button (REQ-NAV-003).
    fs::write(theme.join("zoom.css"), tree::ZOOM_CSS)?;
    fs::write(theme.join("morinet.js"), tree::MORINET_JS)?;

    Ok(ConversionReport {
        build_dir: build,
        pages: entries.len(),
        missing_links,
    })
}

/// Walk the source, skipping excluded dirs and dotfiles.
fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if !is_excluded_dir(&name) {
                collect(root, &path, out)?;
            }
        } else if !name.starts_with('.') {
            out.push(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

/// For every nested folder (2+ path segments) holding `.md` files and lacking
/// a native `index.md`/`_index.md`, write a generated `_section.md` landing
/// page and register it as an entry (mdBook 0.5 rejects draft SUMMARY parents
/// that have children — the parent must be a hyperlink).
fn add_folder_notes(
    src_root: &Path,
    files: &[String],
    entries: &mut Vec<tree::PageEntry>,
) -> anyhow::Result<()> {
    use std::collections::{BTreeMap, BTreeSet};
    let file_set: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    let has_native_index = |dir_prefix: &str| -> Option<&'static str> {
        ["index.md", "_index.md"]
            .iter()
            .find(|n| file_set.contains(&format!("{dir_prefix}{n}")[..]))
            .copied()
    };

    // Nested folder prefixes, deepest chains included.
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    for rel in files {
        if !rel.ends_with(".md") {
            continue;
        }
        let comps: Vec<&str> = rel.split('/').collect();
        for k in 2..comps.len() {
            dirs.insert(comps[..k].join("/"));
        }
    }

    // Snapshot of mirrored pages (rel → title): computed against, while the
    // generated notes are pushed into `entries` itself.
    let snapshot: Vec<(String, String)> = entries
        .iter()
        .map(|e| (e.rel.clone(), e.title.clone()))
        .collect();
    let titles: BTreeMap<&str, &str> = snapshot
        .iter()
        .map(|(rel, title)| (rel.as_str(), title.as_str()))
        .collect();

    for dir in dirs {
        let prefix = format!("{dir}/");
        if has_native_index(&prefix).is_some() {
            continue; // the mirrored native folder note already links children
        }
        let mut pages: Vec<(String, String)> = Vec::new();
        let mut subs: BTreeSet<String> = BTreeSet::new();
        for (rel, title) in &snapshot {
            let rel = rel.as_str();
            if !rel.starts_with(&prefix) {
                continue;
            }
            let rest = &rel[prefix.len()..];
            if !rest.contains('/') {
                pages.push((title.clone(), rest.to_string()));
            } else {
                subs.insert(rest.split('/').next().unwrap().to_string());
            }
        }
        pages.sort();
        let sub_links: Vec<(String, String)> = subs
            .into_iter()
            .map(|sname| {
                let sprefix = format!("{prefix}{sname}/");
                match has_native_index(&sprefix) {
                    Some(n) => {
                        let rel = format!("{sprefix}{n}");
                        let title = titles
                            .get(rel.as_str())
                            .map(|t| (*t).to_string())
                            .unwrap_or_else(|| tree::humanize(&sname));
                        (title, format!("{sname}/{n}"))
                    }
                    None => (tree::humanize(&sname), format!("{sname}/_section.md")),
                }
            })
            .collect();
        let dir_name = dir.split('/').next_back().unwrap_or(&dir).to_string();
        let note = tree::folder_note(&dir_name, &pages, &sub_links);
        std::fs::write(src_root.join(format!("{prefix}_section.md")), note)?;
        entries.push(tree::PageEntry {
            rel: format!("{prefix}_section.md"),
            title: tree::humanize(&dir_name),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Fixture derived from the test name — parallel tests must never share
    /// a temp-dir (pitfall #16: they delete each other's fixtures).
    fn wiki(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("morinet-convert-{tag}"));
        let _ = fs::remove_dir_all(&dir);
        let write = |rel: &str, content: &str| {
            let p = dir.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, content).unwrap();
        };
        write(
            "index.md",
            "---\ntitle: Home\n---\n\n# Home\n\n- [[a|Concept A]]\n",
        );
        write(
            "concepts/a.md",
            "---\ntitle: Concept A\ntype: concept\ntags: [music]\nupdated: 2026-10-20\n---\n\nSee [[b]] and [[gone]].\n",
        );
        write(
            "entities/bands/b.md",
            "---\ntitle: B\n---\n\n# B\n\nBack to [[a]].\n",
        );
        write(
            "raw/article.md",
            "---\nsource_url: https://x.example\nsha256: deadbeef\n---\n\nReal text.\n\n```\n[[literal]] in code\n```\n",
        );
        write("assets/pic.png", "\u{0}binary-stuff");
        write(".obsidian/workspace.json", "{}");
        write("_archive/old.md", "---\ntitle: Old\n---\ngone\n");
        dir
    }

    fn read(dir: &Path, rel: &str) -> String {
        fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    #[test]
    fn detect_covers_all_three_formats() {
        let dir = wiki("detect");
        assert_eq!(detect(&dir), VaultFormat::LlmWiki);
        let native = std::env::temp_dir().join("morinet-convert-detect-native");
        let _ = fs::remove_dir_all(&native);
        fs::create_dir_all(&native).unwrap();
        fs::write(native.join("book.toml"), "[book]\ntitle=\"x\"\n").unwrap();
        assert_eq!(detect(&native), VaultFormat::Native);
        let empty = std::env::temp_dir().join("morinet-convert-detect-empty");
        let _ = fs::remove_dir_all(&empty);
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(detect(&empty), VaultFormat::Neither);
    }

    #[test]
    fn convert_builds_a_complete_mdbook_tree() {
        let dir = wiki("full");
        let report = convert(&dir, "Test Wiki").expect("convert");
        assert_eq!(report.build_dir, dir.join(".morinet-build"));
        let build = &report.build_dir;

        // Skeleton files.
        assert!(read(build, "book.toml").contains("title = \"Test Wiki\""));
        assert!(build.join("theme/morinet.css").is_file());
        let summary = read(build, "src/SUMMARY.md");
        assert!(summary.contains("[Home](index.md)"), "Home link generated");
        assert!(
            summary.contains("[Concept A](concepts/a.md)"),
            "fm title used"
        );

        // Links rewritten against the mirrored layout.
        let a = read(build, "src/concepts/a.md");
        assert!(
            a.contains("[B](../entities/bands/b.md)") || a.contains("[b](../entities/bands/b.md)")
        );
        assert!(a.contains("<span class=\"wikilink-missing\">gone</span>"));
        // Frontmatter consumed: meta block present, raw YAML gone.
        assert!(a.contains("<div class=\"note-meta\">"));
        assert!(
            !a.starts_with("---"),
            "frontmatter must not survive into src"
        );
        assert_eq!(report.missing_links, 1);
        // 4 source pages + 1 generated folder note (entities/bands/).
        assert_eq!(report.pages, 5);

        // Generated folder note is the linked SUMMARY parent (mdBook 0.5 rule).
        let note = read(build, "src/entities/bands/_section.md");
        assert!(note.contains("# Bands"));
        assert!(note.contains("[B](b.md)"), "folder note links its pages");
        assert!(summary.contains("[Bands](entities/bands/_section.md)"));
        assert!(summary.contains("  - [B](entities/bands/b.md)"));

        // Code inside raw/ preserved verbatim.
        let raw = read(build, "src/raw/article.md");
        assert!(raw.contains("[[literal]] in code"));

        // Assets mirrored; excluded trees absent.
        assert!(build.join("src/assets/pic.png").is_file());
        assert!(!build.join("src/_archive").exists());
        assert!(!build.join("src/.obsidian").exists());
    }

    #[test]
    fn convert_never_touches_the_source() {
        let dir = wiki("untouched");
        let _ = convert(&dir, "Test Wiki").unwrap();
        let a = read(&dir, "concepts/a.md");
        assert!(a.contains("[[b]]"), "source keeps its wikilinks");
        assert!(a.starts_with("---"), "source keeps its frontmatter");
        assert!(!dir.join("book.toml").exists(), "no scaffolding in source");
    }

    #[test]
    fn convert_is_fresh_every_time() {
        // A note deleted between syncs must not leave a stale generated page.
        let dir = wiki("fresh");
        let stale = dir.join(".morinet-build/src/ghost.md");
        fs::create_dir_all(stale.parent().unwrap()).unwrap();
        fs::write(&stale, "stale").unwrap();
        convert(&dir, "Test Wiki").unwrap();
        assert!(!stale.exists(), "build dir is regenerated from scratch");
    }

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let path = entry.unwrap().path();
            let dest = to.join(path.file_name().unwrap());
            if path.is_dir() {
                copy_tree(&path, &dest);
            } else {
                fs::copy(&path, &dest).unwrap();
            }
        }
    }

    /// Oracle at the REAL boundary: a living llm-wiki, copied, converted, then
    /// built by the actual upstream `mdbook` binary. Zero broken links allowed.
    /// `MORINET_CORPUS=/path/to/wiki cargo test -- --ignored` (mdbook in PATH).
    #[test]
    #[ignore = "needs mdbook in PATH + MORINET_CORPUS"]
    fn oracle_real_mdbook_builds_the_converted_corpus() {
        let corpus = PathBuf::from(
            std::env::var("MORINET_CORPUS").expect("MORINET_CORPUS must point to a real llm-wiki"),
        );
        let dir = std::env::temp_dir().join("morinet-oracle-mdbook");
        let _ = fs::remove_dir_all(&dir);
        copy_tree(&corpus, &dir);

        let report = convert(&dir, "Oracle Wiki").expect("convert");
        let build_arg = report.build_dir.display().to_string();
        let out = std::process::Command::new("mdbook")
            .args(["build", &build_arg])
            .output()
            .expect("mdbook must be in PATH");
        let log = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !log.contains("Broken link"),
            "mdbook reported broken links:\n{log}"
        );
        assert!(
            report.build_dir.join("book/index.html").is_file(),
            "mdbook did not produce book/index.html:\n{log}"
        );
        println!(
            "oracle mdbook ok: {} pages, {} inert links; book built at {}",
            report.pages,
            report.missing_links,
            report.build_dir.display()
        );
    }
}
