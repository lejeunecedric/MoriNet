//! Bite 1 of Phase 8b — frontmatter: split + parse the llm-wiki YAML subset.
//!
//! Contract (SCHEMA.md of a real llm-wiki, measured on ProgWiki): flat
//! `key: value`, bare dates, inline lists `tags: [a, b]`, optional quoted
//! scalars. Block lists tolerated for Obsidian compat. No YAML typing:
//! everything stays a string; malformed frontmatter → treated as absent.

use std::collections::BTreeMap;

/// One frontmatter value: a scalar string or an inline/block list of strings.
#[derive(Debug, Clone, PartialEq)]
enum Field {
    Scalar(String),
    List(Vec<String>),
}

/// Flat key/value map of frontmatter fields. Values stay strings — no YAML
/// typing (a bare `2026-10-20` must not become a date object).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frontmatter {
    fields: BTreeMap<String, Field>,
}

impl Frontmatter {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn scalar(&self, key: &str) -> Option<&str> {
        match self.fields.get(key)? {
            Field::Scalar(s) => Some(s),
            Field::List(_) => None,
        }
    }

    /// List value for `key`; empty when absent or when the field is a scalar.
    pub fn list(&self, key: &str) -> Vec<String> {
        match self.fields.get(key) {
            Some(Field::List(items)) => items.clone(),
            _ => Vec::new(),
        }
    }

    pub fn title(&self) -> Option<&str> {
        self.scalar("title")
    }

    /// SCHEMA.md: optional `contested: true` marks unresolved contradictions.
    pub fn is_contested(&self) -> bool {
        self.scalar("contested") == Some("true")
    }
}

/// Split a leading `---` frontmatter block from a markdown page.
///
/// Returns `(frontmatter, body)`. No frontmatter (or an unterminated block)
/// → `(empty, whole input)` — the page is never truncated defensively.
pub fn split(md: &str) -> (Frontmatter, &str) {
    if !(md.starts_with("---\n") || md.starts_with("---\r\n")) {
        return (Frontmatter::default(), md);
    }
    // Scan for the closing delimiter line.
    let open_end = md.find('\n').map(|i| i + 1).unwrap_or(md.len());
    let mut p = open_end;
    let close = loop {
        let line_end = md[p..].find('\n').map(|i| p + i + 1).unwrap_or(md.len());
        let line = md[p..line_end].trim_end_matches(['\r', '\n']);
        if line == "---" {
            break (p, line_end);
        }
        if line_end == md.len() {
            // No closing delimiter: treat the whole thing as body.
            return (Frontmatter::default(), md);
        }
        p = line_end;
    };
    let (close_start, close_end) = close;
    (parse(&md[open_end..close_start]), &md[close_end..])
}

/// Parse flat `key: value` lines plus `key:` + indented `- item` block lists.
/// Blank and colon-less lines are ignored.
fn parse(region: &str) -> Frontmatter {
    let mut fields = BTreeMap::new();
    let mut pending: Option<String> = None;
    for raw_line in region.lines() {
        let line = raw_line.trim_end_matches('\r');
        let item = line
            .trim_start()
            .strip_prefix("- ")
            .filter(|_| line.chars().next().is_some_and(char::is_whitespace));
        if let (Some(key), Some(item)) = (pending.as_ref(), item) {
            if let Some(Field::List(items)) = fields.get_mut(key) {
                items.push(unquote(item.trim()));
                continue;
            }
        }
        pending = None;
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = value.trim();
        let field = match inline_list(value) {
            Some(items) => Field::List(items),
            None if value.is_empty() => {
                // Key without value: following `- item` lines fill this list.
                pending = Some(key.to_string());
                Field::List(Vec::new())
            }
            None => Field::Scalar(unquote(value)),
        };
        fields.insert(key.to_string(), field);
    }
    Frontmatter { fields }
}

/// `[a, "b c"]` → parsed items; anything else → None. An empty list is a list.
fn inline_list(value: &str) -> Option<Vec<String>> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    let items = inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(unquote)
        .collect();
    Some(items)
}

/// Strip one layer of matching quotes, if present.
fn unquote(v: &str) -> String {
    let bytes = v.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn splits_frontmatter_from_body() {
        let page = "---\ntitle: King Crimson\ncreated: 2026-10-20\ntype: entity\n---\n\n# King Crimson\n\nBand page.\n";
        let (fm, body) = split(page);
        assert_eq!(fm.title(), Some("King Crimson"));
        assert!(
            body.starts_with("\n# King Crimson"),
            "body must start after the closing ---"
        );
        assert!(
            !body.contains("title:"),
            "frontmatter must not leak into the body"
        );
    }

    #[test]
    fn page_without_frontmatter_is_returned_untouched() {
        let page = "# Just a heading\n\nSome prose.\n";
        let (fm, body) = split(page);
        assert!(fm.is_empty());
        assert_eq!(body, page);
    }

    #[test]
    fn unterminated_block_is_treated_as_absent() {
        // A page that merely starts with a thematic break must not lose content.
        let page = "---\nno closing delimiter here\n--- no either\n";
        let (fm, body) = split(page);
        assert!(fm.is_empty(), "no closing line → no frontmatter");
        assert_eq!(body, page);
    }

    #[test]
    fn closing_delimiter_is_the_first_standalone_dashes_line() {
        // A `---` later in the body (horizontal rule) must not be swallowed.
        let page = "---\ntitle: X\n---\n\nBefore rule.\n\n---\n\nAfter rule.\n";
        let (fm, body) = split(page);
        assert_eq!(fm.title(), Some("X"));
        assert!(body.contains("\n---\n"), "the body hr must survive");
        assert!(body.contains("After rule."));
    }

    #[test]
    fn inline_lists_are_parsed() {
        // Real llm-wiki shape: `tags: [band, history, music]`, bare or quoted items.
        let page = "---\ntitle: Yes\ntags: [band, history, music]\nsources: [raw/articles/yes.md]\nconfidence: \"high\"\n---\n\nBody.\n";
        let (fm, _) = split(page);
        assert_eq!(fm.list("tags"), ["band", "history", "music"]);
        assert_eq!(fm.list("sources"), ["raw/articles/yes.md"]);
        assert_eq!(
            fm.scalar("confidence"),
            Some("high"),
            "quoted scalar is unquoted"
        );
        // A list key is not a scalar, and vice versa.
        assert!(fm.scalar("tags").is_none());
        assert!(fm.list("title").is_empty());
        // An empty inline list yields nothing.
        let (fm2, _) = split("---\ntags: []\n---\n");
        assert!(fm2.list("tags").is_empty());
    }

    #[test]
    fn block_lists_are_parsed() {
        // Obsidian-compatible variant: `key:` then indented `- item` lines.
        let page = "---\ntitle: Zeuhl\ntags:\n  - substyle\n  - history\nsources:\n  - raw/articles/zeuhl.md\n---\n";
        let (fm, _) = split(page);
        assert_eq!(fm.title(), Some("Zeuhl"));
        assert_eq!(fm.list("tags"), ["substyle", "history"]);
        assert_eq!(fm.list("sources"), ["raw/articles/zeuhl.md"]);
    }

    #[test]
    fn crlf_pages_are_handled() {
        let page = "---\r\ntitle: Yes\r\n---\r\n\r\n# Yes\r\n";
        let (fm, body) = split(page);
        assert_eq!(fm.title(), Some("Yes"));
        assert!(body.contains("# Yes"));
    }

    #[test]
    fn contested_flag_reads_as_bool() {
        let (fm, _) = split("---\ntitle: X\ncontested: true\n---\n");
        assert!(fm.is_contested());
        let (fm2, _) = split("---\ntitle: X\ncontested: false\n---\n");
        assert!(!fm2.is_contested());
        let (fm3, _) = split("---\ntitle: X\n---\n");
        assert!(!fm3.is_contested());
    }

    /// Oracle against a REAL vault (not a fixture): the subset parser must
    /// consume every frontmatter block of a living llm-wiki. Run with
    /// `MORINET_CORPUS=/path/to/wiki cargo test -- --ignored` — skipped in CI.
    #[test]
    #[ignore = "needs a real vault directory via MORINET_CORPUS"]
    fn oracle_every_page_of_a_real_corpus_parses_cleanly() {
        let dir =
            std::env::var("MORINET_CORPUS").expect("MORINET_CORPUS must point to a real llm-wiki");
        let mut pages = 0usize;
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                if path.is_dir() {
                    if !name.starts_with('.') && name != "node_modules" {
                        stack.push(path);
                    }
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let md = std::fs::read_to_string(&path).unwrap();
                let has_fm = md.starts_with("---\n");
                let (fm, body) = split(&md);
                if has_fm {
                    assert!(
                        !fm.is_empty(),
                        "frontmatter of {} was not consumed: {:?}",
                        path.display(),
                        &body[..body.len().min(40)]
                    );
                    pages += 1;
                    // SCHEMA.md contract: wiki pages (not raw/) carry a title.
                    if path.to_string_lossy().contains("/raw/") {
                        assert!(
                            fm.scalar("source_url").is_some() || fm.scalar("sha256").is_some(),
                            "raw page {} lacks provenance fields",
                            path.display()
                        );
                    } else {
                        assert!(fm.title().is_some(), "page {} has no title", path.display());
                    }
                }
            }
        }
        assert!(
            pages > 50,
            "corpus too small to be a real oracle: {pages} frontmatter pages"
        );
        println!("oracle ok: {pages} pages parsed");
    }
}
