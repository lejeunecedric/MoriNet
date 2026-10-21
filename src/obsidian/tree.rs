//! Bite 3 of Phase 8b — the generated mdBook skeleton.
//!
//! Pure functions producing the files that turn an llm-wiki tree into a
//! buildable mdBook source: `SUMMARY.md` (folder tree, alphabetical, `index.md`
//! as Home), `book.toml` (with our theme CSS wired, AD-2), and the per-page
//! meta block HTML consumed from frontmatter (`AD-14` decisions D4/D5).

use super::frontmatter::Frontmatter;

/// One page in the generated SUMMARY tree.
#[derive(Debug, Clone, PartialEq)]
pub struct PageEntry {
    /// Vault-relative posix path, e.g. `entities/bands/yes.md`.
    pub rel: String,
    /// Display title (frontmatter `title`, else first H1, else file stem).
    pub title: String,
}

/// Title resolution order (locked D4): frontmatter title → first H1 → stem.
pub fn page_title(fm: &Frontmatter, body: &str, rel: &str) -> String {
    if let Some(t) = fm.title() {
        return t.to_string();
    }
    for line in body.lines() {
        if let Some(h) = line.strip_prefix("# ") {
            let h = h.trim();
            if !h.is_empty() {
                return h.to_string();
            }
        }
    }
    std::path::Path::new(rel)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| rel.to_string())
}

/// Generate `src/SUMMARY.md` from page entries (locked D5: folder tree,
/// alphabetical, `index.md` as Home, regenerated at every sync).
pub fn summary_md(entries: &[PageEntry]) -> String {
    let mut out = String::from("# Summary\n\n");
    if let Some(home) = entries.iter().find(|e| e.rel == "index.md") {
        out.push_str(&format!("[{}]({})\n\n", home.title, home.rel));
    }
    // Root-level pages (not index.md) as plain items.
    let mut roots: Vec<&PageEntry> = entries
        .iter()
        .filter(|e| !e.rel.contains('/') && e.rel != "index.md")
        .collect();
    sort_pages(&mut roots);
    for r in roots {
        out.push_str(&format!("- [{}]({})\n", r.title, r.rel));
    }
    // One section per top-level folder, alphabetical.
    let mut top_dirs: Vec<String> = entries
        .iter()
        .filter(|e| e.rel.contains('/'))
        .map(|e| e.rel.split('/').next().unwrap().to_string())
        .collect();
    top_dirs.sort();
    top_dirs.dedup();
    for dir in top_dirs {
        let prefix = format!("{dir}/");
        let kids: Vec<&PageEntry> = entries
            .iter()
            .filter(|e| e.rel.starts_with(&prefix))
            .collect();
        out.push_str(&format!("\n# {}\n\n", humanize(&dir)));
        render_children(&mut out, &prefix, &kids, 0);
    }
    out
}

fn sort_pages(v: &mut Vec<&PageEntry>) {
    v.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.rel.cmp(&b.rel))
    });
}

/// Emit files directly in `dir_prefix`, then sub-folders as draft parents
/// with nested children (mdBook honours indented lists as sub-chapters).
fn render_children(out: &mut String, dir_prefix: &str, kids: &[&PageEntry], indent: usize) {
    let pad = "  ".repeat(indent);
    let mut files: Vec<&PageEntry> = kids
        .iter()
        .copied()
        .filter(|e| !e.rel[dir_prefix.len()..].contains('/'))
        .collect();
    sort_pages(&mut files);
    for f in files {
        out.push_str(&format!("{pad}- [{}]({})\n", f.title, f.rel));
    }
    let mut subs: Vec<String> = kids
        .iter()
        .filter(|e| e.rel[dir_prefix.len()..].contains('/'))
        .map(|e| {
            format!(
                "{dir_prefix}{}",
                e.rel[dir_prefix.len()..].split('/').next().unwrap()
            )
        })
        .collect();
    subs.sort();
    subs.dedup();
    for s in subs {
        let name = s.split('/').next_back().unwrap_or(&s);
        out.push_str(&format!("{pad}- {}\n", humanize(name)));
        let subkids: Vec<&PageEntry> = kids
            .iter()
            .copied()
            .filter(|e| e.rel.starts_with(&s))
            .collect();
        render_children(out, &format!("{s}/"), &subkids, indent + 1);
    }
}

/// Folder name → section title: capitalize, hyphens to spaces.
fn humanize(dir: &str) -> String {
    let spaced = dir.replace('-', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

/// Generate `book.toml` for the converted vault (AD-2: mdBook upstream +
/// our injected stylesheet, no fork).
pub fn book_toml(vault_name: &str) -> String {
    format!(
        "[book]\ntitle = \"{}\"\nlanguage = \"en\"\nmultilingual = false\nsrc = \"src\"\n\n[output.html]\nadditional-css = [\"theme/morinet.css\"]\n",
        vault_name.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// Escape text for safe interpolation into HTML.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The meta block prepended to a rendered page from its frontmatter (D4):
/// type, tags, dates. Empty frontmatter → no block at all.
pub fn meta_block(fm: &Frontmatter) -> String {
    let mut chips: Vec<String> = Vec::new();
    if let Some(t) = fm.page_type() {
        chips.push(format!("<span class=\"note-type\">{}</span>", esc(t)));
    }
    let tags = fm.list("tags");
    if !tags.is_empty() {
        let joined = tags
            .iter()
            .map(|t| format!("#{}", esc(t)))
            .collect::<Vec<_>>()
            .join(" ");
        chips.push(format!("<span class=\"note-tags\">{joined}</span>"));
    }
    if let Some(u) = fm.scalar("updated") {
        chips.push(format!(
            "<span class=\"note-updated\">upd. {}</span>",
            esc(u)
        ));
    }
    if fm.is_contested() {
        chips.push("<span class=\"note-contested\">⚠ contested</span>".to_string());
    }
    if chips.is_empty() {
        return String::new();
    }
    format!("<div class=\"note-meta\">{}</div>\n\n", chips.join(""))
}

/// Stylesheet copied to the generated tree (`theme/morinet.css`) and wired
/// via `additional-css` — styles the meta block and inert wikilinks.
pub const MORINET_CSS: &str = ".note-meta{display:flex;flex-wrap:wrap;gap:.5rem;font-size:.8rem;opacity:.85;margin-bottom:1rem}\n.note-meta span{border-radius:999px;padding:.1rem .6rem;border:1px solid rgba(128,128,128,.4)}\n.note-type{font-style:italic}\n.note-contested{color:#d33;border-color:#d33}\n.wikilink-missing{opacity:.55;border-bottom:1px dotted currentColor;cursor:help}\n";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obsidian::frontmatter;

    fn e(rel: &str, title: &str) -> PageEntry {
        PageEntry {
            rel: rel.into(),
            title: title.into(),
        }
    }

    #[test]
    fn summary_is_a_folder_tree_home_first_alphabetical() {
        let entries = [
            e("index.md", "Home"),
            e("log.md", "Log"),
            e("concepts/suite.md", "Suite"),
            e("concepts/zeuhl.md", "Zeuhl"),
            e("entities/bands/yes.md", "Yes"),
            e("entities/instruments/mellotron.md", "Mellotron"),
        ];
        let want = "# Summary\n\n\
             [Home](index.md)\n\n\
             - [Log](log.md)\n\n\
             # Concepts\n\n\
             - [Suite](concepts/suite.md)\n\
             - [Zeuhl](concepts/zeuhl.md)\n\n\
             # Entities\n\n\
             - Bands\n  - [Yes](entities/bands/yes.md)\n\
             - Instruments\n  - [Mellotron](entities/instruments/mellotron.md)\n";
        assert_eq!(summary_md(&entries), want);
    }

    #[test]
    fn summary_without_index_has_no_home_link() {
        let out = summary_md(&[e("concepts/a.md", "A")]);
        assert!(!out.contains("(index.md)"));
        assert!(out.starts_with("# Summary"));
    }

    #[test]
    fn title_resolves_fm_then_h1_then_stem() {
        let (fm, _) = frontmatter::split("---\ntitle: King Crimson\n---\n# whatever\n");
        assert_eq!(
            page_title(&fm, "# whatever", "entities/bands/x.md"),
            "King Crimson"
        );
        let (fm2, body2) = frontmatter::split("# Real Heading\n\nProse.\n");
        assert_eq!(page_title(&fm2, body2, "concepts/x.md"), "Real Heading");
        assert_eq!(
            page_title(&fm2, "plain prose", "concepts/odd-time.md"),
            "odd-time"
        );
    }

    #[test]
    fn book_toml_wires_our_theme_css() {
        let toml = book_toml("ProgWiki");
        assert!(toml.contains("title = \"ProgWiki\""));
        assert!(toml.contains("src = \"src\""));
        assert!(toml.contains("additional-css = [\"theme/morinet.css\"]"));
    }

    #[test]
    fn meta_block_shows_type_tags_dates_and_escapes() {
        let (fm, _) = frontmatter::split(
            "---\ntitle: X\ntype: concept\ntags: [music, \"a<b\"]\nupdated: 2026-10-20\n---\n",
        );
        let html = meta_block(&fm);
        assert!(html.starts_with("<div class=\"note-meta\">"));
        assert!(html.contains("concept"));
        assert!(html.contains("#music"));
        assert!(html.contains("a&lt;b"), "tags must be HTML-escaped");
        assert!(html.contains("2026-10-20"));
        // Empty frontmatter → no block.
        let (fm0, _) = frontmatter::split("no fm here");
        assert_eq!(meta_block(&fm0), "");
    }
}
