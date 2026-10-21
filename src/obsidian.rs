//! llm-wiki / Obsidian vault support (Phase 8b — obsidian-format).
//!
//! Pure logic, no axum (same convention as `webdav.rs`): the converter turns a
//! source vault (Obsidian format: bare `[[wikilinks]]`, YAML frontmatter, no
//! `book.toml`) into an mdBook-native tree under `.morinet-build/`, then the
//! normal `mdbook build` runs on that generated tree. The source vault is
//! never modified.
//!
//! Decisions locked 2026-10-21: see `.planning/phases/08b-obsidian-format/`.

pub mod convert;
pub mod frontmatter;
pub mod links;
pub mod tree;
