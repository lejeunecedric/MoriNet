---
one_liner: OKF scan wired into all six register/refresh arms + OKF badge on vault cards and stats panel
requirements_completed: [OKF-01, OKF-02]
status: complete
commit: 89761ba
---

# 18-03 Summary — refresh-scan hook + OKF badge

## What was built

**Scan helper** (src/server.rs:3085):
`pub async fn record_okf_scan(store: &Store, slug: &str, book_dir: &std::path::Path) -> anyhow::Result<()>`
— resolves `webdav::source_dir_of(book_dir)`, skips (`Ok(())`) when the source dir is absent (`is_dir` guard — `source_dir_of` never checks existence), scans with `crate::okf::scan_conformance`, and performs the SINGLE `set_vault_okf(serde_json::to_string(&stats))` write only after a complete scan (D-02: failure leaves previous stats untouched).
Visibility is **`pub`, not `pub(crate)` as the plan said**: main.rs is a separate binary crate consuming the `morinet` lib, so `pub(crate)` would not reach it (plan deviation, forced by crate structure).

**Six best-effort call sites** (all `if let Err(e) = … { eprintln!("okf-scan: scan failed for {slug}: {e}") }`, redirects byte-identical):
1. git refresh `refresh_my_vault` — server.rs:3155
2. personal load `load_personal_vault` — server.rs:3240
3. upload-register (`/vaults/upload` Ok arm) — server.rs:3592
4. `finish_upload_update` success tail — server.rs:3782 (scans `dir`, the live source tree — `source_dir_of(dir)` returns `Some(dir)` for a non-`book` path)
5. admin create — server.rs:3961 (`&p.slug`)
6. background GitHub sync loop — main.rs:226 (`morinet::server::record_okf_scan(&bg_store, slug, &book_dir)`, fires only on seed success via `else if`)

**Badge** (server.rs:529 `fn okf_badge_html(json: Option<&str>) -> String`): serde-defensive (T-18-03: unparseable ⇒ empty); `version: None` ⇒ `<small class="menu-btn">not an OKF bundle</small>`; `Some(v)` ⇒ `<small class="menu-btn">OKF v{v} — {concepts}/{total} concepts</small>` (em-dash U+2014, version html_escaped, counts typed integers — T-18-06). Zero new CSS (`.menu-btn` pill reused; `git diff assets/` empty).
`vault_row` extended to `(a, can_share, okf_badge: &str)` (:569), badge interpolated after `{t}`; both index call sites fetch `state.store.vault_okf(&a.vault.slug).await.ok().flatten()`.
**Stats panel** (owner branch only): `okf_line` local + `<p><strong>OKF conformance:</strong> {text} (index.md: yes/no, log.md: yes/no, scanned {YYYY-MM-DD}).</p>` interpolated after the `</dl>` (server.rs:2993). Date via `epoch_secs_to_utc_date(secs: u64)` (:550).

## TDD evidence

- RED (task 1): `cargo test --lib okf_scan` → `error[E0425]: cannot find function record_okf_scan` (×4) + okf_badge_html/epoch_secs E0425s.
- GREEN (task 1): `test result: ok. 4 passed` (writes_stats, skips_missing_source_dir, fires_on_upload_register, overwrites_on_rerun) → commit 93f5f81.
- RED (task 2): badge tests restored → 6× E0425.
- GREEN (task 2): okf_badge 5 passed, home_page_ 6 passed, epoch_secs 1 passed → commit 89761ba.
- FULL suite: **522 passed / 0 failed** (baseline 511 + 11 new). `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean; `git diff --exit-code Cargo.toml Cargo.lock assets/` clean.

## Acceptance gates

`fn record_okf_scan` ×1; `record_okf_scan(&state.store` ×9 (≥5); `record_okf_scan(&bg_store` ×1 in main.rs; `okf-scan:` ×5 in server.rs (≥3); vault_row signature grep matches; badge literals present; assets/ untouched.

## Deviations

- `pub` instead of `pub(crate)` for `record_okf_scan` (bin crate cannot see `pub(crate)` of the lib crate).
- Task-1 commit temporarily excluded the badge tests (held out so task 1 could compile/GREEN independently); restored verbatim for task 2.
- `epoch_secs_to_utc_date` kept as a standalone reference civil-from-days implementation instead of delegating to the existing `fmt_date` — see Lessons.

## Decisions

- `finish_upload_update` passes `dir` (the live uploads source tree) directly to `record_okf_scan`; `source_dir_of` returns it unchanged (not a `…/book` path), so the scan targets the mutated source per D-02.
- Background sync scans only on seed success (`else if`), matching "seed_vault success" in the plan.
- Stats line placed after `</dl>` (valid HTML; `<p>` cannot live inside `<dl>`), owner branch only — sharee branch untouched (T-10.1-07).

## Lessons

- **Latent bug found in existing `fmt_date` (server.rs ~:1278):** it computes day-of-month from `doe` instead of `doy` (and uses `yoe / 146` where the reference algorithm needs `yoe / 100`), e.g. `fmt_date(1762400000)` → `"2025-11-9137"`. Out of scope for 18-03 (not fixed); `epoch_secs_to_utc_date` is standalone and correct (unit-tested). Worth a follow-up phase/hotfix.
- The patch tool's lint pass dumps the whole file's edition-2015 false errors on a 9.7k-line file (~100KB per call) — route edits to giant files through `execute_code` + `hermes_tools.patch` and print only the success flag.
- `source_dir_of` existence-blindness is exactly why the `is_dir` guard matters; the skip test (`okf_scan_helper_skips_missing_source_dir`) is the regression pin for it.

## notes_for_next_executor

- `record_okf_scan` is **`pub`** in `morinet::server` (not `pub(crate)`) — main.rs already calls it; 18-04 can reuse it freely.
- Badge helpers: `okf_badge_html(Option<&str>) -> String` (private, server.rs:529) and `epoch_secs_to_utc_date(u64) -> String` (private, :550).
- Route-table: 18-03 added **no routes** — `build_router` untouched; 18-04's export route inserts in the `.route(...)` chain in `build_router` (server.rs ~:200-260) like any other handler; no conflicts introduced.
- Existing `fmt_date` is buggy (see Lessons) — do not build new date rendering on it.
