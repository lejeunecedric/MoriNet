//! mdBook preprocessor subcommand that strips OKF YAML frontmatter at
//! render time (D-05).
//!
//! mdBook does NOT strip YAML frontmatter itself (reproduced on mdbook
//! v0.5.4: the block renders as visible text), yet the docs vault pages must
//! keep it on disk for OKF validators. This module is wired via the docs
//! vault's `book.toml` (`[preprocessor.okf-frontmatter]`) and covers BOTH
//! mdbook build paths (`sources.rs` refresh and the `server.rs` WebDAV-PUT
//! rebuild), since both run `mdbook build <src>` on the same on-disk
//! book.toml.
//!
//! Protocol (verified against mdbook v0.5.4): mdBook spawns the command,
//! first as `<cmd> supports <renderer>` — the preprocessor MUST exit 0 there
//! or mdBook silently SKIPS it and frontmatter leaks with no error. Then it
//! passes stdin as a JSON array `[context, book]` and expects the modified
//! **Book object** (`{"items": [...]}` — NOT the array) on stdout.

use anyhow::{Context, Result};
use serde_json::Value;

/// True when argv (after the subcommand name) is the mdBook `supports`
/// probe (`supports <renderer>`). The CLI glue must exit 0 for this call.
pub fn is_supports_probe(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("supports")
}

/// Strip a leading YAML frontmatter block from chapter content.
///
/// Only strips when the FIRST line is exactly `---` and a LATER line is
/// exactly `---`; everything else is left byte-identical (no-op when there
/// is no frontmatter; an unterminated opening fence is left alone).
pub fn strip_frontmatter(content: &str) -> String {
    // Opening fence: the very first line must be exactly `---`.
    let rest = match content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))
    {
        Some(rest) => rest,
        None => return content.to_string(),
    };
    // Closing fence: a later line that is exactly `---`; strip through it
    // plus its trailing newline. No closing fence → leave untouched.
    let mut idx = 0usize;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == "---" {
            return rest[idx + line.len()..].to_string();
        }
        idx += line.len();
    }
    content.to_string()
}

/// Preprocessor core: parse the `[context, book]` JSON array mdBook sends
/// on stdin, strip frontmatter from every Chapter (recursing into
/// `sub_items`), and return the serialized **Book object** bytes.
pub fn preprocess(input: &[u8]) -> Result<Vec<u8>> {
    let parsed: Value = serde_json::from_slice(input)
        .context("mdbook-okf-preprocessor: stdin is not valid JSON")?;
    let book = match &parsed {
        Value::Array(arr) if arr.len() >= 2 => &arr[1],
        _ => anyhow::bail!("mdbook-okf-preprocessor: expected a [context, book] JSON array"),
    };
    let mut book = book.clone();
    if let Some(items) = book.get_mut("items").and_then(Value::as_array_mut) {
        strip_items(items);
    }
    let out = serde_json::to_vec(&book).context("mdbook-okf-preprocessor: re-serializing book")?;
    Ok(out)
}

/// Recursively strip frontmatter from every `{"Chapter": ...}` item.
fn strip_items(items: &mut [Value]) {
    for item in items.iter_mut() {
        let Some(chapter) = item.get_mut("Chapter") else {
            continue;
        };
        if let Some(content) = chapter.get_mut("content") {
            if let Some(text) = content.as_str() {
                *content = Value::String(strip_frontmatter(text));
            }
        }
        if let Some(sub) = chapter.get_mut("sub_items").and_then(Value::as_array_mut) {
            strip_items(sub);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `[context, book]` stdin payload with the given chapter items.
    fn stdin_json(items: Value) -> Vec<u8> {
        let payload = serde_json::json!([
            {"root": "/tmp/src", "config": {"book": {"title": "T"}}},
            {"items": items}
        ]);
        serde_json::to_vec(&payload).unwrap()
    }

    fn chapter(content: &str) -> Value {
        serde_json::json!({
            "Chapter": {
                "name": "Quick start",
                "content": content,
                "number": [1],
                "sub_items": [],
                "path": "quickstart.md",
                "source_path": "src/quickstart.md",
                "parent_names": []
            }
        })
    }

    fn first_content(output: &[u8]) -> String {
        let book: Value = serde_json::from_slice(output).unwrap();
        book["items"][0]["Chapter"]["content"]
            .as_str()
            .unwrap()
            .to_string()
    }

    const FM: &str =
        "---\ntype: Guide\ntitle: Quick start\ngenerated:\n  by: process:morinet-docs\n---\n";

    #[test]
    fn frontmatter_is_stripped() {
        let out = preprocess(&stdin_json(serde_json::json!([chapter(&format!(
            "{FM}# Quick start\n\nbody text\n"
        ),)])))
        .unwrap();
        assert_eq!(first_content(&out), "# Quick start\n\nbody text\n");
    }

    #[test]
    fn page_without_frontmatter_is_unchanged() {
        let body = "# Quick start\n\n---\n\na thematic break stays\n";
        let out = preprocess(&stdin_json(serde_json::json!([chapter(body)]))).unwrap();
        assert_eq!(first_content(&out), body);
    }

    #[test]
    fn unterminated_fence_is_unchanged() {
        let body = "---\ntype: Guide\n# no closing fence\n";
        let out = preprocess(&stdin_json(serde_json::json!([chapter(body)]))).unwrap();
        assert_eq!(first_content(&out), body);
    }

    #[test]
    fn nested_sub_items_are_stripped() {
        let mut parent = chapter("# Parent\n");
        let child = chapter(&format!("{FM}# Child\n"));
        parent["Chapter"]["sub_items"] = serde_json::json!([child]);
        let out = preprocess(&stdin_json(serde_json::json!([parent]))).unwrap();
        let book: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            book["items"][0]["Chapter"]["sub_items"][0]["Chapter"]["content"].as_str(),
            Some("# Child\n")
        );
    }

    #[test]
    fn output_is_book_object_not_array() {
        let out = preprocess(&stdin_json(serde_json::json!([chapter("x\n")]))).unwrap();
        let parsed: Value = serde_json::from_slice(&out).unwrap();
        assert!(parsed.is_object(), "must serialize the Book object");
        assert!(parsed.get("items").is_some(), "Book object must keep items");
    }

    #[test]
    fn invalid_json_fails_loudly() {
        assert!(preprocess(b"not json").is_err());
        // A single object (not the [context, book] array) must also fail.
        assert!(preprocess(br#"{"items":[]}"#).is_err());
    }

    #[test]
    fn supports_probe_is_detected() {
        assert!(is_supports_probe(&["supports".into(), "html".into()]));
        assert!(!is_supports_probe(&[]));
        assert!(!is_supports_probe(&["build".into()]));
    }
}
