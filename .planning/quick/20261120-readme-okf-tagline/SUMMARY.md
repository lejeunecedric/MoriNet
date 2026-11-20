---
status: complete
date: 2026-11-20
one_liner: README tagline now presents llm-wiki/Obsidian vaults and OKF bundles as first-class served content
---

# Summary: README tagline — foreground OKF import/read

Replaced the README.md:3 tagline per operator request: OKF bundles moved from "and portable OKF bundles" (export-sounding tail) to a first-class item alongside llm-wiki / Obsidian vaults in the "exposes ... as shared wiki sites" clause. The existing markdown link on "OKF bundles" (spec repo URL) was kept to preserve README link style.

- Verified `grep -rn "Self-hosted mdBook server"` — README.md:3 was the only occurrence repo-wide; no docs/Cargo.toml/lib.rs copies to sync.
- No tests affected (docs-only change).
