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
/// Make a title/alias safe as `[link text](target)` (mdBook's SUMMARY parser
/// rejects raw `[` in titles): inline markdown links collapse to their display
/// text (`[Rockasteria](http://x/)` → `Rockasteria`), leftover brackets are
/// escaped (`[draft] x` → `\[draft\] x`).
pub fn md_link_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        let Some(close) = rest.find(']') else {
            break;
        };
        if rest[close + 1..].starts_with('(') {
            if let Some(end) = rest[close + 1..].find(')') {
                let end = close + 1 + end;
                // [text](url) → text
                out.push_str(&rest[1..close]);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push_str("\\[");
        rest = &rest[1..];
    }
    for c in rest.chars() {
        match c {
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            _ => out.push(c),
        }
    }
    out
}

pub fn summary_md(entries: &[PageEntry]) -> String {
    let mut out = String::from("# Summary\n\n");
    if let Some(home) = entries.iter().find(|e| e.rel == "index.md") {
        out.push_str(&format!(
            "[{}]({})\n\n",
            md_link_text(&home.title),
            home.rel
        ));
    }
    // Root-level pages (not index.md) as plain items.
    let mut roots: Vec<&PageEntry> = entries
        .iter()
        .filter(|e| !e.rel.contains('/') && e.rel != "index.md")
        .collect();
    sort_pages(&mut roots);
    for r in roots {
        out.push_str(&format!("- [{}]({})\n", md_link_text(&r.title), r.rel));
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
        out.push_str(&format!("{pad}- [{}]({})\n", md_link_text(&f.title), f.rel));
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
        // The parent item must be a real hyperlink (mdBook 0.5): the folder's
        // native index page, else the generated `_section.md` landing page.
        let parent = ["index.md", "_index.md", "_section.md"]
            .iter()
            .find_map(|n| kids.iter().find(|e| e.rel == format!("{s}/{n}")));
        match parent {
            Some(p) => out.push_str(&format!("{pad}- [{}]({})\n", md_link_text(&p.title), p.rel)),
            None => out.push_str(&format!(
                "{pad}- {}\n",
                humanize(s.split('/').next_back().unwrap_or(&s))
            )),
        }
        let subkids: Vec<&PageEntry> = kids
            .iter()
            .copied()
            .filter(|e| {
                e.rel.starts_with(&s) && Some(e.rel.as_str()) != parent.map(|p| p.rel.as_str())
            })
            .collect();
        render_children(out, &format!("{s}/"), &subkids, indent + 1);
    }
}

/// Folder name → section title: capitalize, hyphens to spaces.
pub fn humanize(dir: &str) -> String {
    let spaced = dir.replace('-', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

/// Generate `book.toml` for the converted vault (AD-2: mdBook upstream +
/// our injected stylesheet, no fork).
///
/// Only fields valid across mdBook 0.4 AND 0.5: `multilingual` was REMOVED in
/// 0.5 (caught by the real-binary oracle — `mdbook build` on the corpus fails
/// on unknown fields now). Omitting it is valid everywhere; the default is
/// already single-language.
pub fn book_toml(vault_name: &str) -> String {
    format!(
        "[book]\ntitle = \"{}\"\nlanguage = \"en\"\nsrc = \"src\"\n\n[output.html]\nadditional-css = [\"theme/morinet.css\", \"theme/zoom.css\"]\nadditional-js = [\"theme/morinet.js\"]\n",
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
/// Zoom toolbar + "Close vault" button injected into every served mdBook page
/// (`assets/zoom.js`): zooms content + sidebar in lockstep and links back to
/// the app root. The source of truth is the asset file, not a baked copy.
pub const MORINET_JS: &str = include_str!("../../assets/zoom.js");

/// The toolbar styles (`assets/zoom.css`): one fixed bar at the top right,
/// below mdBook's print button, holding zoom controls + Close vault.
pub const ZOOM_CSS: &str = include_str!("../../assets/zoom.css");

pub const MORINET_CSS: &str = r#"/*
 * Formatting of the served mdBook pages (post-critique fixes):
 * - comfortable reading line-height (mdBook's 1.45 is tight)
 * - the meta chips are real-sized text, never faded (opacity kills contrast)
 * - missing wikilinks stay fully readable: dotted underline, no opacity fade
 * - body font matches the app (system stack) instead of the default Open Sans
 * - dark-mode icons meet the 3:1 non-text contrast minimum
 */
.content p, .content li, .content dd { line-height: 1.65; }
body, .content { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }

.note-meta {
  display: flex;
  flex-wrap: wrap;
  gap: .5rem;
  font-size: 13px;
  margin-bottom: 1rem;
}
.note-meta span {
  border-radius: 999px;
  padding: .15rem .7rem;
  border: 1px solid rgba(128, 128, 128, .4);
}
.note-type { font-style: italic; }
.note-contested { color: #c5221f; border-color: #c5221f; }
.wikilink-missing {
  border-bottom: 1px dotted currentColor;
  cursor: help;
}
@media (prefers-color-scheme: dark) {
  :root { --icons: #7a8290; }
  .note-contested { color: #f28b82; border-color: #f28b82; }
}
"#;

/// Landing page for a sub-folder that has no native index — mdBook requires
/// nested SUMMARY chapters to hang under a real hyperlink (measured with
/// mdbook 0.5: a draft parent with children fails `Summary parsing`).
pub fn folder_note(
    dir_name: &str,
    pages: &[(String, String)],
    subs: &[(String, String)],
) -> String {
    let mut out = format!("# {}\n\n", humanize(dir_name));
    if !pages.is_empty() {
        out.push_str("Pages in this section:\n\n");
        for (title, href) in pages {
            out.push_str(&format!("- [{}]({href})\n", md_link_text(title)));
        }
        out.push('\n');
    }
    if !subs.is_empty() {
        out.push_str("Subsections:\n\n");
        for (title, href) in subs {
            out.push_str(&format!("- [{}]({href})\n", md_link_text(title)));
        }
        out.push('\n');
    }
    out
}

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
    fn summary_survives_titles_with_markdown_links() {
        // Real B3Wiki case (2026-10-21): pages whose H1 is itself a link
        // (`# [Rockasteria](http://rockasteria.blogspot.com/)`) produced
        // `- [[Rockasteria](url)](path)` and mdBook refused to parse the
        // summary ("Expected a start of a link, actually got Text("[")").
        // Titles collapse to their display text; leftover brackets escape.
        let pages = vec![
            PageEntry {
                rel: "index.md".to_string(),
                title: "B3Wiki".to_string(),
            },
            PageEntry {
                rel: "raw/articles/rockasteria.md".to_string(),
                title: "[Rockasteria](http://rockasteria.blogspot.com/)".to_string(),
            },
            PageEntry {
                rel: "raw/notes.md".to_string(),
                title: "[draft] gig notes".to_string(),
            },
        ];
        let summary = summary_md(&pages);
        assert!(
            summary.contains("- [Rockasteria](raw/articles/rockasteria.md)"),
            "link-typed title collapses to its text:\n{summary}"
        );
        assert!(
            summary.contains("- [\\[draft\\] gig notes](raw/notes.md)"),
            "plain brackets are escaped:\n{summary}"
        );
        assert!(
            !summary.contains("- [["),
            "no raw double bracket:\n{summary}"
        );
    }

    #[test]
    fn page_title_prefers_frontmatterree_home_first_alphabetical() {
        // Sub-folders nest under a LINKED parent (mdBook 0.5 rejects draft
        // parents with children — caught by the real-binary oracle).
        let entries = [
            e("index.md", "Home"),
            e("log.md", "Log"),
            e("concepts/suite.md", "Suite"),
            e("concepts/zeuhl.md", "Zeuhl"),
            e("entities/bands/_section.md", "Bands"),
            e("entities/bands/yes.md", "Yes"),
            e("entities/instruments/_section.md", "Instruments"),
            e("entities/instruments/mellotron.md", "Mellotron"),
        ];
        let want = "# Summary\n\n\
             [Home](index.md)\n\n\
             - [Log](log.md)\n\n\
             # Concepts\n\n\
             - [Suite](concepts/suite.md)\n\
             - [Zeuhl](concepts/zeuhl.md)\n\n\
             # Entities\n\n\
             - [Bands](entities/bands/_section.md)\n  - [Yes](entities/bands/yes.md)\n\
             - [Instruments](entities/instruments/_section.md)\n  - [Mellotron](entities/instruments/mellotron.md)\n";
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
        assert!(toml.contains("additional-css = [\"theme/morinet.css\", \"theme/zoom.css\"]"));
        // The zoom + close-vault button need the JS injected (AD-2).
        assert!(toml.contains("additional-js = [\"theme/morinet.js\"]"));
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
