//! Bite 2 of Phase 8b — wikilink resolution à l'Obsidian.
//!
//! Contract (locked 2026-10-21):
//! - `[[slug]]` resolves to the first `**/slug.md` match: root first, then the
//!   shallowest directory, then alphabetical. Case-insensitive fallback.
//! - `[[target|Alias]]` keeps the alias as display text.
//! - `[[page#Heading]]` → anchor kebab-cased to mdBook's heading ids.
//! - `![[img.png]]` → a markdown image embed of the mirrored file.
//! - An unresolved link → inert `<span class="wikilink-missing">Text</span>`,
//!   never a link, never a 404.
//! - Rewriting is code-fence aware: ```` ``` ```` blocks, inline code spans and
//!   HTML comments are never touched (real `raw/` pages contain literal `[[…]]`).
//!
//! Links are emitted relative to the page that contains them, so mdBook's own
//! relative-`.md` → `.html` rewriting honours them — the generated SUMMARY.md
//! and the mirrored tree share one folder layout.

use std::collections::BTreeMap;
use std::path::Path;

/// The set of `.md`/asset files a vault contains, addressed Obsidian-style.
#[derive(Debug, Clone, Default)]
pub struct VaultIndex {
    /// rel paths (posix, vault-relative) of every mirrorable file.
    pub files: Vec<String>,
    /// lowercase slug → the best matching `.md` rel path.
    slug_targets: BTreeMap<String, String>,
}

/// The resolution of one wikilink target.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    /// A link target: rel path from the vault root (`.md` page).
    Page(String),
    /// An embed target: any non-`.md` file (image, pdf…).
    Asset(String),
    /// Nothing matched — render inert.
    Missing,
}

impl VaultIndex {
    /// Build an index from the mirrored file list (vault-relative, posix).
    pub fn build(files: Vec<String>) -> Self {
        let mut slug_targets: BTreeMap<String, String> = BTreeMap::new();
        // Sort shallow-first then alphabetical so the first insert wins ties
        // the same way Obsidian does (closest to root).
        let mut pages: Vec<&String> = files.iter().filter(|f| f.ends_with(".md")).collect();
        pages.sort_by_key(|f| (f.matches('/').count(), f.as_str()));
        for f in pages {
            let stem = Path::new(f)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            slug_targets
                .entry(stem.to_lowercase())
                .or_insert_with(|| f.clone());
            // Also index the full path (minus .md) so `[[entities/yes]]` works.
            slug_targets
                .entry(f.trim_end_matches(".md").to_lowercase())
                .or_insert_with(|| f.clone());
        }
        VaultIndex {
            files,
            slug_targets,
        }
    }

    /// Resolve a raw wikilink target (without alias/section) from `from_page`.
    pub fn resolve(&self, target: &str, _from_page: &str) -> Resolved {
        let t = target.trim();
        if t.is_empty() {
            return Resolved::Missing;
        }
        let lower = t.to_lowercase();
        // 1. Exact vault-relative path, any file type.
        if let Some(f) = self.files.iter().find(|f| f.to_lowercase() == lower) {
            return if f.ends_with(".md") {
                Resolved::Page(f.clone())
            } else {
                Resolved::Asset(f.clone())
            };
        }
        // 2. Bare slug (or path minus .md) → the indexed page.
        let key = lower.trim_end_matches(".md").to_string();
        if let Some(p) = self.slug_targets.get(&key) {
            return Resolved::Page(p.clone());
        }
        // 3. Asset by bare file name, Obsidian shortest-path style.
        if let Some(f) = self.files.iter().find(|f| {
            Path::new(f)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase())
                == Some(lower.clone())
        }) {
            return Resolved::Asset(f.clone());
        }
        // 4. Obsidian: `[[Some Title]]` may match a slug after kebab-casing.
        let kebab = kebab(&key);
        if let Some(p) = self.slug_targets.get(&kebab) {
            return Resolved::Page(p.clone());
        }
        Resolved::Missing
    }
}

/// `Yes vs Genesis` → `yes-vs-genesis` (mdBook lowercases and dashes heading ids).
pub fn kebab(s: &str) -> String {
    s.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c.to_lowercase().next().unwrap_or(c))
            } else if c == ' ' || c == '_' || c == '-' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        // collapse repeated dashes and trim edges
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Make `target` (vault-relative) reachable from `from_page` (also relative).
/// Pure string path arithmetic: `entities/bands/yes.md` → `concepts/suite.md`
/// becomes `../../concepts/suite.md`.
pub fn rel_link(from_page: &str, target: &str) -> String {
    let depth = Path::new(from_page)
        .parent()
        .map(|p| p.components().count())
        .unwrap_or(0);
    let mut out = String::new();
    for _ in 0..depth {
        out.push_str("../");
    }
    out.push_str(target);
    out
}

/// Rewrite every wikilink in `body` of page `from_page` against `index`.
///
/// Line-based scanner: ``` and ~~~ fences, inline code spans and HTML comments
/// are copied verbatim — literal `[[…]]` inside them is content, not a link.
pub fn rewrite(from_page: &str, body: &str, index: &VaultIndex) -> String {
    let mut out = String::with_capacity(body.len());
    let mut in_fence = false;
    let mut in_comment = false;
    for line in body.split_inclusive('\n') {
        let t = line.trim_start();
        if !in_comment && !t.is_empty() && (t.starts_with("```") || t.starts_with("~~~")) {
            in_fence = !in_fence;
            out.push_str(line);
            continue;
        }
        if in_fence {
            out.push_str(line);
            continue;
        }
        if in_comment {
            match line.find("-->") {
                Some(end) => {
                    out.push_str(&line[..end + 3]);
                    let rest = &line[end + 3..];
                    if !rest.is_empty() {
                        out.push_str(&rewrite_line(rest, from_page, index));
                    }
                    in_comment = false;
                }
                None => out.push_str(line),
            }
            continue;
        }
        // A comment opened without closing on this line: keep state.
        if let Some(start) = line.find("<!--") {
            if line[start..].find("-->").is_none() {
                out.push_str(&rewrite_line(&line[..start], from_page, index));
                out.push_str(&line[start..]);
                in_comment = true;
                continue;
            }
        }
        out.push_str(&rewrite_line(line, from_page, index));
    }
    out
}

/// Rewrite links in one non-fenced line, preserving inline code and comments.
fn rewrite_line(line: &str, from_page: &str, index: &VaultIndex) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while !rest.is_empty() {
        let tick = rest.find('`');
        let comment = rest.find("<!--");
        let link = rest.find("[[");
        let Some(n) = [tick, comment, link].into_iter().flatten().min() else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..n]);
        rest = &rest[n..];
        if rest.starts_with('`') {
            // Inline code span: copy through the matching backtick run.
            let run = rest.chars().take_while(|c| *c == '`').count();
            let open = &rest[..run];
            match rest[run..].find(open) {
                Some(c) => {
                    let end = run + c + run;
                    out.push_str(&rest[..end]);
                    rest = &rest[end..];
                }
                None => {
                    out.push_str(open);
                    rest = &rest[run..];
                }
            }
        } else if rest.starts_with("<!--") {
            let end = rest.find("-->").map(|e| e + 3).unwrap_or(rest.len());
            out.push_str(&rest[..end]);
            rest = &rest[end..];
        } else {
            // A wikilink; `!` glued in front (already emitted) = embed.
            let embed = out.ends_with('!');
            if embed {
                out.pop();
            }
            let inner_rest = &rest[2..];
            let Some(end) = inner_rest.find("]]") else {
                // Unterminated `[[` at end of line: emit verbatim, stop.
                out.push_str(if embed { "![[" } else { "[[" });
                out.push_str(inner_rest);
                break;
            };
            let inner = &inner_rest[..end];
            rest = &inner_rest[end + 2..];
            out.push_str(&render_link(inner, from_page, index, embed));
        }
    }
    out
}

/// Render one wikilink (content between `[[` and `]]`).
fn render_link(inner: &str, from_page: &str, index: &VaultIndex, embed: bool) -> String {
    let (target, alias) = match inner.split_once('|') {
        Some((t, a)) => (t.trim(), Some(a.trim())),
        None => (inner.trim(), None),
    };
    let (target, section) = match target.split_once('#') {
        Some((t, s)) => (t.trim(), Some(s.trim())),
        None => (target, None),
    };
    // Default display: section, else file name (stem), not the folder path.
    let fallback = match section {
        Some(s) => s.to_string(),
        None => Path::new(target.trim_end_matches(".md"))
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| target.to_string()),
    };
    // Link text must not carry raw brackets (same class as the SUMMARY title
    // bug) — collapse/escape them through the shared sanitizer.
    let display = crate::obsidian::tree::md_link_text(alias.unwrap_or(fallback.as_str()));
    let anchor = section
        .map(|s| format!("#{}", kebab(s)))
        .unwrap_or_default();
    match index.resolve(target, from_page) {
        Resolved::Page(p) => {
            let href = format!("{}{anchor}", rel_link(from_page, &p));
            // `![[page]]` transclusion is out of scope v1 → a plain link.
            format!("[{display}]({href})")
        }
        Resolved::Asset(a) => {
            let href = rel_link(from_page, &a);
            if embed {
                format!("![{display}]({href})")
            } else {
                format!("[{display}]({href})")
            }
        }
        Resolved::Missing => {
            format!("<span class=\"wikilink-missing\">{display}</span>")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(files: &[&str]) -> VaultIndex {
        VaultIndex::build(files.iter().map(|s| s.to_string()).collect())
    }

    // Tracer bullet — a bare `[[slug]]` becomes a relative md link.
    // Default display text = the target as written (Obsidian parity); the
    // corpus mostly uses explicit aliases.
    #[test]
    fn alias_with_brackets_is_escaped_as_link_text() {
        // Same class as the SUMMARY title bug: link text must not carry raw
        // `[`/`]` or mdBook's parser refuses the page.
        let index = idx(&["index.md", "entities/x.md"]);
        let out = rewrite("index.md", "see [[x|[draft] the notes]] here", &index);
        assert!(
            out.contains("[\\[draft\\] the notes]"),
            "brackets escaped in the alias: {out}"
        );
    }

    #[test]
    fn bare_slug_becomes_relative_md_link() {
        let index = idx(&["index.md", "entities/bands/yes.md", "concepts/suite.md"]);
        let out = rewrite("concepts/suite.md", "See [[yes]] for the band.", &index);
        assert_eq!(out, "See [yes](../entities/bands/yes.md) for the band.");
    }

    #[test]
    fn alias_is_used_as_display_text() {
        // The real corpus shape: `[[jimmy-smith|Jimmy Smith]]` (542/812 links).
        let index = idx(&["index.md", "entities/players/jimmy-smith.md"]);
        let out = rewrite(
            "index.md",
            "Organ trio: [[jimmy-smith|Jimmy Smith]] plays.",
            &index,
        );
        assert_eq!(
            out,
            "Organ trio: [Jimmy Smith](entities/players/jimmy-smith.md) plays."
        );
    }

    #[test]
    fn path_targets_show_their_stem() {
        // Obsidian displays the file name, not the path, when no alias is given.
        let index = idx(&["index.md", "entities/bands/yes.md"]);
        let out = rewrite("index.md", "[[entities/bands/yes]] rocks.", &index);
        assert_eq!(out, "[yes](entities/bands/yes.md) rocks.");
    }

    #[test]
    fn resolution_is_case_insensitive() {
        let index = idx(&["entities/bands/yes.md", "concepts/suite.md"]);
        assert_eq!(
            index.resolve("YES", "concepts/suite.md"),
            Resolved::Page("entities/bands/yes.md".into())
        );
    }

    #[test]
    fn ties_resolve_root_first_then_shallowest() {
        // Obsidian picks the file closest to the vault root.
        let index = idx(&[
            "entities/genesis.md",
            "entities/bands/genesis.md",
            "concepts/genesis.md",
        ]);
        assert_eq!(
            index.resolve("genesis", "index.md"),
            Resolved::Page("concepts/genesis.md".into())
        );
        let with_root = idx(&["genesis.md", "entities/genesis.md"]);
        assert_eq!(
            with_root.resolve("genesis", "index.md"),
            Resolved::Page("genesis.md".into())
        );
    }

    #[test]
    fn section_links_get_kebab_anchors() {
        // `[[suite#Extended composition]]` → mdBook heading id is kebab-cased.
        let index = idx(&["concepts/suite.md", "entities/bands/yes.md"]);
        let out = rewrite(
            "entities/bands/yes.md",
            "Form: [[suite#Extended composition]].",
            &index,
        );
        assert_eq!(
            out,
            "Form: [Extended composition](../../concepts/suite.md#extended-composition)."
        );
    }

    #[test]
    fn broken_link_renders_inert_never_a_link() {
        let index = idx(&["index.md"]);
        let out = rewrite("index.md", "See [[mellotron|Mellotron]] here.", &index);
        assert_eq!(
            out,
            "See <span class=\"wikilink-missing\">Mellotron</span> here."
        );
        assert!(
            !out.contains('('),
            "no link syntax may leak for a missing page"
        );
    }

    #[test]
    fn embeds_become_image_includes() {
        let index = idx(&["index.md", "assets/king-crimson.jpg"]);
        let out = rewrite("concepts/x.md", "![[king-crimson.jpg]]", &index);
        assert_eq!(out, "![king-crimson.jpg](../assets/king-crimson.jpg)");
        let aliased = rewrite("index.md", "![[king-crimson.jpg|KC live]]", &index);
        assert_eq!(aliased, "![KC live](assets/king-crimson.jpg)");
        // A broken embed degrades to the same inert span.
        let broken = rewrite("index.md", "![[gone.png]]", &index);
        assert_eq!(broken, "<span class=\"wikilink-missing\">gone.png</span>");
    }

    #[test]
    fn code_fences_and_inline_code_are_not_rewritten() {
        // Real `raw/` pages contain literal `[[…]]` inside code.
        let index = idx(&["entities/bands/yes.md", "index.md"]);
        let body =
            "Text [[yes]].\n\n```md\nlet s = \"[[yes]]\"; // literal\n```\n\nInline `[[no]]` stays.\n\n~~~\n[[yes]] in tilde fence\n~~~\n";
        let out = rewrite("index.md", body, &index);
        assert!(
            out.contains("[yes](entities/bands/yes.md)"),
            "prose link rewritten"
        );
        assert!(out.contains("\"[[yes]]\""), "fenced literal preserved");
        assert!(out.contains("`[[no]]`"), "inline code preserved");
        assert!(
            out.contains("[[yes]] in tilde fence"),
            "tilde fence preserved"
        );
    }

    /// Only the text OUTSIDE fences/inline code — for the corpus oracle.
    fn unprotected(body: &str) -> String {
        let mut out = String::new();
        let mut in_fence = false;
        for line in body.lines() {
            let t = line.trim_start();
            if t.starts_with("```") || t.starts_with("~~~") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence {
                continue;
            }
            // drop inline code spans (naive backtick pairing is enough here)
            let mut keep = String::new();
            let mut in_code = false;
            for c in line.chars() {
                if c == '`' {
                    in_code = !in_code;
                    continue;
                }
                if !in_code {
                    keep.push(c);
                }
            }
            out.push_str(&keep);
            out.push('\n');
        }
        out
    }

    /// Oracle against a REAL vault (not a fixture): every wikilink of a
    /// living llm-wiki must become a resolved link or an inert span — no
    /// literal `[[…]]` may survive outside code.
    #[test]
    #[ignore = "needs a real vault directory via MORINET_CORPUS"]
    fn oracle_every_wikilink_of_a_real_corpus_is_rewritten() {
        use crate::obsidian::frontmatter;
        let dir = std::path::PathBuf::from(
            std::env::var("MORINET_CORPUS").expect("MORINET_CORPUS must point to a real llm-wiki"),
        );
        let root = dir.canonicalize().unwrap();
        let mut files: Vec<String> = Vec::new();
        let mut pages: Vec<(String, String)> = Vec::new();
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                if path.is_dir() {
                    if !name.starts_with('.') && name != "_archive" {
                        stack.push(path);
                    }
                    continue;
                }
                let rel = path
                    .canonicalize()
                    .unwrap()
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push(rel.clone());
                if name.ends_with(".md") {
                    pages.push((rel, std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        let index = VaultIndex::build(files);
        let mut rewritten_links = 0usize;
        let mut missing = 0usize;
        for (rel, md) in &pages {
            let (_, body) = frontmatter::split(md);
            let out = rewrite(rel, body, &index);
            let leftover = unprotected(&out);
            assert!(
                !leftover.contains("[["),
                "page {rel}: literal [[ survived outside code:\n{}",
                leftover.lines().find(|l| l.contains("[[")).unwrap_or("")
            );
            rewritten_links += out.matches("](").count();
            missing += out.matches("wikilink-missing").count();
        }
        assert!(
            rewritten_links > 600,
            "expected hundreds of resolved links, got {rewritten_links}"
        );
        println!(
            "oracle ok: {} pages, {rewritten_links} links rewritten, {missing} inert missing",
            pages.len()
        );
    }
}
