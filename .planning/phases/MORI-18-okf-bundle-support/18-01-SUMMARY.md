---
id: "18-01"
one_liner: "src/okf.rs core module — pure OKF conformance scan, §5.3 trust tier, note_meta JSON builder, Frontmatter::has accessor"
requirements_completed: [OKF-01]
status: complete
commit: 2485600
---

# 18-01 Summary — okf-core-pure-scan-and-trust-tier

## What was built

`src/okf.rs` (new, registered as `pub mod okf;` in `src/lib.rs` after `oidc`):

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OkfStats {
    pub version: Option<String>,   // Some("0.2") / Some("0.1") / None (= not OKF)
    pub concepts: usize,
    pub total: usize,
    pub has_index: bool,
    pub has_log: bool,
    pub scanned_at: u64,           // epoch seconds
}

pub fn scan_conformance(dir: &std::path::Path) -> anyhow::Result<OkfStats>
pub fn trust_tier(fm: &crate::obsidian::frontmatter::Frontmatter) -> &'static str
pub fn note_meta(fm: &crate::obsidian::frontmatter::Frontmatter) -> serde_json::Value
pub fn frontmatter_of(bytes: &[u8]) -> Option<crate::obsidian::frontmatter::Frontmatter>
```

`src/obsidian/frontmatter.rs` — one additive accessor (D-04 compliant, parse() untouched):

```rust
pub fn has(&self, key: &str) -> bool   // fields.contains_key(key) — presence only
```

Behavioral notes for downstream plans:
- `scan_conformance` walks via `crate::mcp::list_md_files` (hidden dirs skipped); unreadable/non-UTF-8 files count in `total` only; reserved = exactly root-relative `"index.md"`/`"log.md"` (case-sensitive), presence-only, excluded from denominators; threshold literal `concepts * 100 >= total * 80`; v0.2 presence set `["generated","verified","sources","status","stale_after"]` checked via `fm.has()` **only on typed concepts**.
- `trust_tier`: `!fm.has("verified")` → `"unverified"`; else any candidate (`list("verified")` items ∪ `scalar("verified")`) containing `"human:"` → `"human-reviewed"`; else `"machine-confirmed"`. Block-list-of-maps items arrive flat (`"by: human:alice"`) so the substring check works.
- `note_meta` shape: `{"type": …|null, "tags": […], "status": …|null, "trust_tier": "…"}`.
- `frontmatter_of`: non-UTF-8, absent, or empty frontmatter all → `None`.

## TDD evidence

- **RED** (tests written first, functions absent): `cargo test --lib okf` → 17 × E0425 `cannot find function scan_conformance / trust_tier / note_meta / frontmatter_of in this scope` — "could not compile morinet (lib test) due to 17 previous errors".
- **GREEN**: `cargo test --lib okf` → 13 passed / 0 failed. Tests: `scan_v02_bundle`, `scan_v01_bundle`, `scan_below_80_percent_is_not_okf`, `scan_exactly_80_percent_is_okf`, `scan_empty_dir_is_not_okf`, `scan_skips_hidden_dirs_and_counts_unreadable`, `scan_reserved_files_excluded_from_denominator`, `trust_tier_absent_verified_is_unverified`, `trust_tier_machine_actor_is_machine_confirmed`, `trust_tier_human_actor_is_human_reviewed`, `trust_tier_scalar_verified_human`, `note_meta_shape_nulls_for_missing_fields`, `frontmatter_of_none_without_frontmatter`.
- Pin tests: `cargo test --lib frontmatter` → 10 passed / 1 ignored (convert.rs "frontmatter must not survive into src" green — D-09).
- Full suite: **506 passed / 0 failed / 4 ignored** (baseline 493 + 13 new).
- `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean; `git diff --exit-code Cargo.toml Cargo.lock` clean — zero new dependencies.

## Deviations

None. Plan followed exactly (OkfStats shape, threshold literal, reserved-file rule, trust algorithm, tmpdir fixture pattern `morinet-okf-{tag}` per mcp.rs:265-270).

## Decisions

- v0.2 presence keys are checked only on **typed** (concept) files, per D-03's "any concept carries …" wording — an untyped file carrying `status:` does not flip the version.
- `trust_tier` candidate set = list items ∪ scalar, so both `verified: human:bob` (scalar) and block-list-of-maps forms derive correctly through the existing flat parser.

## Lessons

- The patch tool's inline linter runs rustc in edition 2015 and floods false E0670 errors on this crate's `async fn` code — ignore it; `cargo` is the source of truth.
- `Frontmatter::has()` was genuinely required: an empty block-list field (`verified:` with indented map lines that leak as top-level keys) is invisible to both `scalar()` and `list()` when the items land under leaked nested keys — but note items DO land in the list here; `has()` still matters for `generated:`/`sources:` maps whose items may be empty.
- `crate::mcp::list_md_files` is `pub(crate)` — usable from okf.rs without any visibility change.
