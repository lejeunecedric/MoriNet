---
phase: MORI-18-okf-bundle-support
review_range: cf96ab9..HEAD (2485600, 7310be8, 93f5f81, 89761ba, a6a3965, 5f601de, 2ddfd71)
verdict: APPROVE WITH FINDINGS
date: 2026-11-06
counts:
  critical: 0
  high: 0
  medium: 4
  low: 4
  info: 4
---

# Phase 18 Review — okf-bundle-support

**Verdict: APPROVE WITH FINDINGS.** No invariant violation of the locked
decisions (D-01..D-10): ACL gates every exposure surface, the 404 collapse
holds on API/MCP/export, caps are checked before buffering, default `/api/v1`
shapes are pinned byte-identical, `trust_tier` is display-only, stored
`okf_json` is serde-parsed defensively in every consumer, and no secrets are
logged. `cargo test okf` → 36 passed / 0 failed locally.

## Findings

| # | Sev | File:Line | Summary |
|---|-----|-----------|---------|
| M-01 | MEDIUM | src/server.rs:3049, src/okf.rs:190-193 | Export failure leaks absolute FS paths; missing source returns 400 not 404 |
| M-02 | MEDIUM | src/okf.rs:181-186 | Dot-**files** are exported (only dot-dirs excluded) — `.env`-style files reach every reader |
| M-03 | MEDIUM | src/server.rs:3047, src/okf.rs:158-221 | Blocking sync FS (≤256 MiB read + deflate) on the async worker; no rate limit on repeated exports |
| M-04 | MEDIUM | src/okf.rs:188-194 | One hostile/broken symlink in a git-synced source permanently breaks export for all readers (whole-bundle bail) |
| L-01 | LOW | src/okf.rs:195-213 | Internal symlinks produce duplicate zip entries (same rel twice) and drop the link name |
| L-02 | LOW | src/api.rs:327-334 | `okf` field embeds arbitrary valid JSON (not parsed as `OkfStats`); N+1 query per vault-list request |
| L-03 | LOW | src/mcp.rs:160-186, 236-253 | Filtered `list_notes` reads every file in the vault (unpaginated) per call — O(n) read amplification |
| L-04 | LOW | src/server.rs:3843 | `finish_upload_update` passes the source dir where the param is documented as `book_dir`; breaks if a source dir is literally named `book` |
| I-01 | INFO | src/api.rs:443-465 | `?meta=1` list re-reads up to 200 files per request (bounded by page cap — acceptable, documented) |
| I-02 | INFO | src/server.rs:549-563 | `epoch_secs_to_utc_date` verified correct across 0..2100 range; correctly avoids the known `fmt_date` doy/doe bug |
| I-03 | INFO | src/okf.rs:140-147 | Caps math overflow-safe (`usize`/`u64`, checked pre-buffer per T-18-10); no zip read-back server-side, so no zip-bomb surface |
| I-04 | INFO | src/okf.rs:74 | `concepts * 100 >= total * 80` — no realistic overflow on 64-bit usize |

## Details

### M-01 — Export error path leaks paths and breaks the documented 404 collapse
`okf_export` (src/server.rs:3049) renders `build_bundle_zip` errors verbatim
into `error_page("export failed: {e}")`. The symlink-escape bail
(src/okf.rs:190-193) embeds `path.display()` — the **resolved absolute path
outside the vault** (e.g. `/home/otheruser/secret.md`), disclosing server FS
layout to any vault reader. Additionally, a *missing* source dir fails
`src.canonicalize()?` (src/okf.rs:163) and answers **400**, contradicting the
doc comment at src/server.rs:3018-3020 ("missing source … collapse to ONE
404", T-18-09). No enumeration leak to non-readers (the access gate runs
first), but the collapse is not what the code claims.
**Fix:** `eprintln!` the detailed error; return the collapsed 404 (or a
generic 500 with no `{e}` interpolation) from `okf_export`; drop
`path.display()` from the bail message in `build_bundle_zip`.

### M-02 — Dot-files are exported
The walk skips dot-**directories** (src/okf.rs:181-186) but exports dot-files
at any depth: `.env`, `.netrc`, `.aws`-style files committed into a vault
source ship inside the bundle to every reader with read ACL. D-06 literally
says "dot-directories", so this is spec-conformant — but the asymmetry is
almost certainly unintended and the blast radius is credential exposure.
**Fix:** skip `name.starts_with('.')` for files too (one-line change in the
walk), or record the intent explicitly in D-06.

### M-03 — Blocking FS + unbounded concurrency on export
`okf_export` calls `build_bundle_zip` synchronously inside the async handler:
up to 256 MiB of `fs::read` + deflate on a tokio worker, whole zip buffered in
a `Vec<u8>` (per plan — acceptable for small vaults), with no rate limit or
dedup. Repeated `GET /vault/{slug}/okf-export.zip` from one reader multiplies
256 MiB-per-request memory and blocks workers.
**Fix:** wrap in `tokio::spawn_blocking`; add a cheap per-user export rate
limit (the existing rate-limit surface if any) and/or lower `MAX_EXPORT_BYTES`
to match the real vault quota.

### M-04 — Whole-bundle bail on a single bad symlink
`build_bundle_zip` `bail`s on the first symlink that resolves outside the
root (src/okf.rs:189) and `?`-propagates `canonicalize` failure on broken
symlinks. A git-synced vault (git preserves symlinks) can carry one
`link -> /etc/passwd` or a dangling link and permanently disable export for
**all** readers of that vault — fail-closed is right for the bytes, wrong for
availability. (A `link -> .` directory cycle is *not* a hang: the walk
terminates at PATH_MAX when `canonicalize` errors — verified empirically.)
**Fix:** skip offending entries (log slug + rel, not the resolved path) and
keep building; only fail if zero entries survive.

### L-01 — Duplicate zip entries via internal symlinks
`rel` is derived from the **canonicalized** path (src/okf.rs:195-202), so
`link.md -> a.md` (both inside the vault) emits `a.md` twice and never
`link.md`; a symlinked dir re-walks its target and duplicates every entry.
Zip tolerates duplicate names but extraction becomes ambiguous.
**Fix:** keep a `HashSet<String>` of emitted rels; skip duplicates.

### L-02 — `okf` field embeds raw JSON; N+1 in vault list
`api_vaults` (src/api.rs:327-334) parses stored `okf_json` as
`serde_json::Value`, so any *valid* JSON (e.g. `5`, `[1,2]`) would be
embedded under `okf`, unlike the typed `OkfStats` parse every other consumer
uses. Data is self-written so risk is low. Each listed vault also costs one
`vault_okf` query.
**Fix:** parse as `crate::okf::OkfStats` and re-serialize (or reuse
`okf_badge_html`'s typed path); batch with a single `WHERE slug IN (...)`.

### L-03 — MCP filter read amplification
`note_matches` (src/mcp.rs) does one `read_note` per candidate; `list_notes`
is unpaginated, so a filtered call reads every `.md` in the vault. Bounded by
vault quota, but it is a cost-amplification vector on an agent-facing surface.
Acceptable for now; note for a future cache (frontmatter is already parsed at
scan time — could be persisted per note).

### L-04 — `record_okf_scan(store, slug, dir)` with a source dir
At src/server.rs:3843 (`finish_upload_update`) the third arg is the upload
**source** dir, while the parameter is documented as `book_dir`
(src/server.rs:3146-3150). It works only because `source_dir_of` passes
non-`book` paths through (src/webdav.rs:75-78); a source dir literally named
`book` would scan the parent. Rename the param to `book_or_source_dir` or
pass the build dir for consistency.

### INFO notes
- I-01: `?meta=1` list cost is bounded by the page cap (200) — documented at
  src/api.rs:443-448; fine.
- I-02: `epoch_secs_to_utc_date` cross-checked against Python's
  `datetime.utcfromtimestamp` for 0, 1970-03-05, 2038, 2100 boundaries —
  exact match. The comment correctly flags the pre-existing `fmt_date`
  doy/doe bug and does **not** rely on it anywhere new.
- I-03: caps checked on metadata before buffering (T-18-10 satisfied);
  `bytes + len` cannot realistically overflow u64.
- I-04: 80% threshold integer math safe.

## Invariant audit (all PASS)

| Invariant | Evidence |
|-----------|----------|
| ACL before exposure (T-18-15) | MCP `note_matches` runs only on post-ACL `crate::api` results (src/mcp.rs:236+); API meta branches run after `read_note`/404 collapse (src/api.rs:443, 543); export gated by `access_for` + collapsed 404 (src/server.rs:3028) |
| 404-not-403 | `export_zip_requires_access` / `export_zip_unknown_vault_404` / `api_meta_inaccessible_vault_404` / `mcp_filters_never_resurrect_inaccessible_vault` all green; `vault_okf` collapses unknown slug (src/store.rs) |
| No traversal outside source | canonicalize + `starts_with(root)` guard (src/okf.rs:188-194) + `bundle_zip_rejects_symlink_escape` test |
| Caps before buffering (T-18-10) | `check_export_caps(entries+1, bytes+len)` before `fs::read` (src/okf.rs:207-208) |
| Default v1.2 byte-identical | strict `meta == "1"` opt-in; `api_list_notes_default_shape_unchanged`, `api_read_note_default_raw_markdown`, `api_vaults_no_okf_key_when_never_scanned` pin it |
| trust_tier never authorization | grep: only `note_meta` display path; comment at src/okf.rs:93 |
| Serde-defensive stored JSON | badge/stats parse `OkfStats` with `.ok()` → render nothing; API parse-with-skip; `api_vaults_okf_field_absent_on_corrupt_json` green |
| D-02 source-not-book | `record_okf_scan` resolves via `source_dir_of` (src/server.rs:3151); all six arms + main.rs bg-sync verified |
| D-06 single archive crate | Cargo.toml: `zip = { version = "9", default-features = false, features = ["deflate"] }` only |
| D-09 converter untouched | no diff in src/obsidian/convert.rs |
| No secrets in logs | scan failures log slug only; export errors not logged with content |

**Baseline:** `cargo test okf` → 36 passed / 0 failed (full suite not re-run).
