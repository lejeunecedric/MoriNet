---
one_liner: OKF bundle export — GET /vault/{slug}/okf-export.zip streams an ACL-gated in-memory zip with virtual root index.md
requirements_completed: [OKF-03]
status: complete
commit: 2ddfd71
---

# 18-04 Summary — okf-export-zip-route

## What was built

- **Dependency (D-06/D-10):** `zip = { version = "9", default-features = false, features = ["deflate"] }`
  (resolved 9.4.1) added to Cargo.toml + Cargo.lock, and the Phase 17 "ONLY new runtime
  dependency" comment rescoped to Phase 17 (openidconnect) + Phase 18 (zip) **in the same commit**.
- **Builder (`src/okf.rs`):**
  - `pub const MAX_EXPORT_ENTRIES: usize = 20_000;` / `pub const MAX_EXPORT_BYTES: u64 = 268_435_456;` (256 MiB)
  - `pub(crate) fn check_export_caps(entries: usize, bytes: u64) -> anyhow::Result<()>` — pure guard,
    Err once a running total reaches a cap; called per file on the **metadata length before buffering** (T-18-10).
  - `pub fn build_bundle_zip(src: &std::path::Path) -> anyhow::Result<Vec<u8>>` — explicit-stack walk of
    `src`; dot-directories skipped (covers `.morinet-build/`, `.git/`); each file canonicalized and
    `starts_with(root)` asserted — symlink escape bails (T-18-08); deflate entries, unix_permissions 0o644,
    `/`-separated relative entry names; virtual root `index.md` (`---\nokf_version: "0.2"\n---\n\n# Bundle index\n`)
    added in-memory **only when the walk found no root index.md** — never written to disk (D-06).
- **Route (`src/server.rs`):** `.route("/vault/{slug}/okf-export.zip", get(okf_export))` inserted beside
  the stats route, before the `{*rest}` catch-all. Handler `okf_export`: `store.access_for` gate
  (Err ⇒ invisible), then book_path from the same `accessible_vaults` row the stats panel uses,
  `webdav::source_dir_of` — store-error / unknown slug / no access / missing source all collapse to the
  identical `(404, "vault not found")` shape, **never 403** (Phase 16 rule, PATTERNS §6 divergence, T-18-09).
  Success: `content-type: application/zip` + `content-disposition: attachment; filename="{slug}-okf.zip"`
  (slug defensively sanitized to alphanumeric/dash) + raw bytes; builder error → `error_page`.
- **Stats panel:** `<a class="btn-ghost" href="/vault/{slug}/okf-export.zip">Export OKF bundle</a>`
  after the Source `<dd>` row (slug html_escaped like every other interpolation).

## TDD evidence

- **RED 1 (task 1):** tests first → `error: could not compile` (build_bundle_zip / check_export_caps / zip crate not found).
- **GREEN 1:** `cargo test --lib bundle_zip` → 5 passed; `export_caps` → 1 passed.
- **RED 2 (task 2):** route absent → 4 failed (`export_zip_requires_access`, `export_zip_unknown_vault_404`,
  `export_zip_streams_bundle`, `export_link_in_stats_panel`).
- **GREEN 2:** `cargo test --lib export` → 5 passed (incl. caps guard).
- **Full suite:** `cargo test --lib` → **548 passed / 0 failed / 4 ignored** (baseline 538/0/4 + 10 new tests).
  convert.rs tests green (D-09 pin intact). `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean.

## Dependency audit (T-18-11 gate)

- `cargo tree -p zip | grep -Ei "bzip|zstd|xz|lzma"` → **empty**; `cargo tree -e normal | grep -iE 'bzip2|zstd|xz|lzma'` → **empty**.
- zip 9.4.1 tree: flate2 (zlib-rs backend) + zopfli + crc32fast/indexmap/getrandom — no TLS, no banned codecs.
- Exactly ONE new runtime dependency in the milestone (zip); `git show --stat` = Cargo.toml, Cargo.lock, src/okf.rs, src/server.rs only.

## Deviations

- None material. Two micro-additions within the plan's discretion: okf.rs module doc updated to mention the
  permitted zip dep (the old "no new dependencies" line became stale), and caps are checked on file metadata
  **before** reading bytes (strictly stronger than the plan's "per file" wording, same guard function).
- zip 9 API note: `ZipFile::name()` returns `Result<Cow<str>, ZipError>` — test helpers unwrap it.

## Decisions

- Followed the plan's discretion calls verbatim (caps constants, guard semantics Ok at MAX-1 / Err at MAX,
  404 collapse, filename sanitization); no re-decisions.
- Handler gets book_path from `accessible_vaults` (same row as stats panel) after the `access_for` read gate —
  the plan's "same accessible entry the stats handler uses" for the path, `access_for` for the gate.

## Lessons

- zip 9's reader API is fallible (`name()` → Result) — differs from zip 2.x muscle memory.
- The patch-tool lint harness runs rustfmt without the crate edition; "async fn not permitted in Rust 2015"
  errors from it are false positives — trust `cargo fmt --check` / `cargo clippy` instead.
- `check_export_caps(entries + 1, bytes + len)` on metadata before `fs::read` keeps the DoS bound honest
  without buffering a hostile file first.
