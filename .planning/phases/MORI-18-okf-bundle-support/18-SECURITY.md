---
phase: 18-okf-bundle-support
verdict: SECURED
date: 2026-11-06
head: a965aa8
auditor: security-audit (GSD)
threats: {closed: 15, accepted: 2, open: 0}
new_findings: {medium: 1, low: 4, info: 2}
---

# Phase 18 Security Audit — okf-bundle-support

**Verdict: SECURED.** All 17 planned threats are CLOSED (15) or ACCEPTED-documented (2);
none OPEN. One new medium finding is a *correction to an accepted-risk statement*
(N-18-01), not an unclosed plan threat. Full suite at head: **549 passed / 0 failed /
4 ignored** (21.93s), matching 18-REVIEW-FIX.

## Threat table

| ID | STRIDE / sev | Status | Mitigation (file:line) | Test evidence (all run at head) |
|---|---|---|---|---|
| T-18-01 | DoS / med | CLOSED | `scan_conformance` pure, best-effort: unreadable/non-UTF-8 → total-only (`src/okf.rs:63-65`); walk tolerates `read_dir` errors (`src/mcp.rs:44-45`); no panic paths | `scan_skips_hidden_dirs_and_counts_unreadable`, `scan_empty_dir_is_not_okf` — pass (9-pass batch) |
| T-18-02 | Spoofing / med | CLOSED (accepted-by-design) | `trust_tier` pure derivation (`src/okf.rs:94-105`); grep confirms call sites are ONLY `note_meta` display + tests — no auth code path consumes it; ACLs stay in `store.access_for` | `trust_tier_*` (4 tests) pin the derivation — pass |
| T-18-03 | Tampering / low | CLOSED | store treats column as opaque TEXT (`src/store.rs:976-994`, no SQL parsing); badge serde-falls-back to empty (`src/server.rs:534-536`); API `okf` key absent on parse Err (`src/api.rs:330-334`) | `vault_okf_roundtrip`, `okf_badge_html_unparseable_is_empty`, `api_vaults_okf_field_absent_on_corrupt_json` — pass |
| T-18-04 | Info Disclosure / low | CLOSED | unknown slug and never-scanned both collapse to `Ok(None)` (`src/store.rs:987-994`) | `vault_okf_none_for_unknown_slug`, `vault_okf_none_when_never_scanned` — pass |
| T-18-05 | DoS / med | CLOSED | `record_okf_scan` best-effort: missing source → `Ok(())` early (`src/server.rs:3162-3167`); callers only `eprintln` (`src/server.rs:3228-3230`); single write after complete scan (`src/server.rs:3169-3172`) | `okf_scan_helper_skips_missing_source_dir` + badge/home batch — pass (7) |
| T-18-06 | Tampering / med | CLOSED | badge built from typed integers + `html_escape(&v)` on the version string (`src/server.rs:539-545`, `html_escape` at `:1201-1207` escapes `& < > "`); unparseable → empty | `okf_badge_html_v02_string_exact`, `okf_badge_html_unparseable_is_empty`, `home_page_shows_okf_badge` — pass. **Test gap:** no test pins escaping of a hostile version string (e.g. hand-edited DB `version:"<script>"`); mitigation is code-verified only |
| T-18-07 | Info Disclosure / low | CLOSED | badge renders only in `vault_row` over `accessible_vaults` output (`src/server.rs:641,660`); no new visibility axis | `home_page_no_badge_when_never_scanned` + existing ACL suite green (full run) |
| T-18-08 | Info Disclosure / **high** | CLOSED | post-fix (cde7995): symlinks **never followed** — lstat via `DirEntry::file_type()`, skip+log (`src/okf.rs:191-195`); non-regular files excluded (`:200-202`); canonicalize + `starts_with(root)` defense-in-depth (`:204-211`); **zip entry names derive from `canon.strip_prefix(&root)` components (`:212-219`) so `..` cannot appear in a name**; walk root always `webdav::source_dir_of(book_path)` from DB, never request (`src/server.rs:3033-3043`); dot-files/dot-dirs skipped (`:182-186`) | `bundle_zip_skips_symlink_escape` (bundle still builds, secret absent), `bundle_zip_skips_dotfiles_and_dotdirs`, `bundle_zip_entries_byte_identical` — pass (10-pass batch incl. export route tests). Residual: hardlinks — see N-18-04 |
| T-18-09 | Info Disclosure / med | CLOSED | single `not_found()` closure: store error, no-access, unknown slug, missing source all → `404 "vault not found"` (`src/server.rs:3026-3050`); builder failure → bare 500, detail only to stderr (`:3055-3062`, REV-18 M-01) | `export_zip_requires_access`, `export_zip_unknown_vault_404` assert identical body — pass |
| T-18-10 | DoS / med | CLOSED | `MAX_EXPORT_ENTRIES=20_000` / `MAX_EXPORT_BYTES=256 MiB` (`src/okf.rs:134-136`); `check_export_caps` runs on `metadata().len()` **before** `fs::read` buffers (`:224-226`); walk aborts on first breach, no partial zip | `export_caps_guard_direct` (constants pinned + boundary), `bundle_zip_entries_byte_identical` — pass. Concurrency caveat → N-18-01 |
| T-18-11 | Tampering / low | CLOSED | `zip` with deflate-only features; `cargo tree -p zip` grep for `bzip|zstd|xz|lzma|rustls|native-tls|openssl` → **0 matches** (tree: indexmap/libc/memchr/typed-path/zopfli/flate2 stack only) | gate re-run at head: 0 matches |
| T-18-12 | Info Disclosure / **high** | CLOSED | list: ACL-gated `list_notes` runs first, 404 problem+json before the meta branch (`src/api.rs:384-396` vs `:449`); read: ACL-gated `read_note` first, 404 collapse before meta (`src/api.rs:522-539` vs `:545`); meta per-entry re-reads go through the same `read_note` (`:452`) | `api_meta_inaccessible_vault_404`, `api_meta_param_non_one_is_default` — pass (6-pass batch) |
| T-18-13 | Tampering / med | CLOSED | meta strictly `== "1"` opt-in (`src/api.rs:449,545`); default shapes re-pinned | `api_list_notes_default_shape_unchanged`, `api_read_note_default_raw_markdown` — pass; full v1.2 suite untouched and green |
| T-18-14 | DoS / low | ACCEPTED | meta list bounded by page cap (limit default 50 / hard cap 200, `src/api.rs:344-345,407`); documented in handler doc-comment (`src/api.rs:446-448`) | statement accurate — cap verified in code |
| T-18-15 | Info Disclosure / **high** | CLOSED | filters run strictly on the Vec returned by ACL-gated `crate::api::list_notes`/`search_notes` (`src/mcp.rs:233-256,293-317`); `note_matches` re-reads via ACL-gated `read_note` (`:176`) — inaccessible vaults never enter the candidate set | `mcp_filters_never_resurrect_inaccessible_vault`, `list_notes_filter_by_type`, `list_notes_filter_type_and_tags_and`, `search_notes_filter_by_type` — pass (4) |
| T-18-16 | DoS / low | ACCEPTED | one extra `read_note` per candidate, bounded by search cap / page sizes; documented in `note_matches` doc-comment (`src/mcp.rs:163-164`) | statement accurate; same cost class as accepted L-03 |
| T-18-17 | Tampering / low | CLOSED | `frontmatter_of` returns `None` on non-UTF-8/absent/empty (`src/okf.rs:120-128`); `note_matches` has **no unwrap** (`src/mcp.rs:165-187`); `frontmatter.rs` unwraps are all `unwrap_or` (safe) | `list_notes_filter_by_type` (no-frontmatter fixture), `list_notes_filter_type_and_tags_and` (empty-not-error) — pass |

## New-threat hunt findings

### N-18-01 — MEDIUM — accepted-risk statement for M-03 rate-limit half is incomplete (memory amplification)
`18-REVIEW-FIX.md` accepts no rate limit because "caps … cannot exceed caps **per call**".
True per call, but **concurrent** exports multiply: each `spawn_blocking` export buffers
up to 256 MiB source + the growing zip output (~2× peak per call), and tokio's blocking
pool defaults to 512 threads. One authenticated user firing ~100 parallel
`/vault/<slug>/okf-export.zip` requests against a single large accessible vault can
force tens of GiB of RSS → OOM. No dedup/semaphore exists. Not re-litigating the rate
limit itself — the **accepted statement should be corrected** to acknowledge
concurrency-multiplied memory, and a per-user export semaphore (or global in-flight cap)
recommended if the instance hosts untrusted users. Severity medium (authenticated,
caps-bounded per call, requires read ACL).

### N-18-02 — LOW — accepted statement inaccurate: `okf` API field embeds arbitrary JSON, not parsed `OkfStats`
`18-REVIEW-FIX.md` claims "`okf` field only embeds parsed `OkfStats`"; the code parses
`serde_json::Value` and embeds it verbatim (`src/api.rs:331-332`) — which matches
18-05-PLAN's own wording but contradicts the fix report. Impact low: the column is only
written by `record_okf_scan` with serde-generated `OkfStats`, so hostile content
requires prior DB write access. Fix the doc statement or tighten to `OkfStats` parse.

### N-18-03 — LOW — log injection via vault filenames into `eprintln`
`src/okf.rs:193,207` print `path.display()`; a filename containing `\n`/`\r`/ANSI
sequences (legal on Linux) forges stderr log lines. Slug interpolation
(`src/server.rs:3058`) is safe — slug is only printed after `access_for` passes, and
vault slug rules are `[a-z0-9-]`. stderr-only sink, no structured log pipeline → low.

### N-18-04 — LOW — hardlinks inside the vault source are bundled
`build_bundle_zip` lstat-skips symlinks, but a **hardlink** is a regular file:
`file_type().is_file()` passes and `canonicalize()` does not collapse hardlinks, so a
hardlink to an out-of-vault file ships its content. Precondition: attacker needs local
FS write access in the vault dir as the server user (git/rsync-from-git sync cannot
create hardlinks; `fs.protected_hardlinks` restricts unprivileged linking) → low.

### N-18-05 — LOW/INFO — recursive symlink dir in scan walk is bounded, not a hang
`list_md_files` follows symlinked dirs via `path.is_dir()` with no visited-set
(`src/mcp.rs:54-57`). Live probe at head: vault with `concepts/loop -> ..` →
`scan_conformance` returned `Ok((41,41))` in **670 µs** — kernel `MAXSYMLINKS` (40)
kills path resolution (`read_dir` Err → `continue`). Bounded ~40× list amplification,
no hang, no unbounded memory. Informational; a depth cap would make the bound explicit.

### N-18-06 — INFO — zip-bomb read-back by downstream consumers
Bundle entries are byte-identical source files (deflate re-compression only); total
uncompressed ≤ 256 MiB enforced pre-buffer, so any downstream unzip is bounded by the
same 256 MiB. No amplification introduced. Informational.

### N-18-07 — INFO — timing variance across the collapsed 404 paths
`okf_export` answers identical 404 bodies for all invisible cases, but the
missing-source path does extra `source_dir_of` + `is_dir` work vs the early
`access_for` bail — microsecond-scale timing could weakly distinguish "exists but
source missing" from "unknown slug". Not practically exploitable over HTTP; informational.

### Checked and clean (no finding)
- **meta=1 amplification:** bounded by page cap (T-18-14 accepted, cap verified).
- **MCP filter read amplification:** accepted L-03, bounded by caps.
- **Path leakage in new error responses:** export failure = bare 500 (`src/server.rs:3059`); API meta errors reuse the existing problem+json shapes; `Content-Disposition` filename sanitized to `[a-z0-9-]` (`src/server.rs:3065-3074`).
- **`..` in zip names:** impossible — names built from canonicalized-path components.
- **Export route auth:** inside the `protected` `route_layer` (`src/server.rs:258,262`).

## Evidence — test runs at head a965aa8

```
cargo test --lib -- scan_skips_hidden_dirs_and_counts_unreadable scan_empty_dir_is_not_okf \
  trust_tier vault_okf_roundtrip vault_okf_none_for_unknown_slug vault_okf_none_when_never_scanned
  → 9 passed; 0 failed
cargo test --lib -- bundle_zip export_caps_guard_direct export_zip
  → 10 passed; 0 failed
cargo test --lib -- okf_badge_html home_page_shows_okf_badge home_page_no_badge_when_never_scanned \
  okf_scan_helper_skips_missing_source_dir
  → 7 passed; 0 failed
cargo test --lib -- api_meta_inaccessible_vault_404 api_list_notes_default_shape_unchanged \
  api_read_note_default_raw_markdown api_vaults_okf_field_absent_on_corrupt_json \
  api_vaults_okf_field_when_scanned api_meta_param_non_one_is_default
  → 6 passed; 0 failed
cargo test --lib -- mcp_filters_never_resurrect_inaccessible_vault list_notes_filter_by_type \
  list_notes_filter_type_and_tags_and search_notes_filter_by_type
  → 4 passed; 0 failed
cargo test --lib (full)
  → 549 passed; 0 failed; 4 ignored; finished in 21.93s
cargo tree -p zip | grep -icE 'bzip|zstd|xz|lzma|rustls|native-tls|openssl'
  → 0
symlink-loop probe (scratch crate, scan_conformance on `concepts/loop -> ..`)
  → Ok((41, 41)) in 670µs (bounded by MAXSYMLINKS; no hang)
```

## Verdict

**SECURED** — 15 CLOSED / 2 ACCEPTED / 0 OPEN. Follow-ups (non-blocking): correct the
two inaccurate accepted-risk statements (N-18-01 concurrency memory, N-18-02 OkfStats
embed), add a badge version-escape test, consider a per-user export semaphore.

## Addendum (same day)

**N-18-01 FIXED** (commit `9568cd3`): global `EXPORT_SEMAPHORE` (2 permits) in
`src/server.rs` queues concurrent exports instead of buffering them in
parallel; test `export_waits_for_semaphore_permit` proves queueing.
Worst-case concurrent export memory now bounded at ~1 GiB.
