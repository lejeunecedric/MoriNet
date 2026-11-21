---
phase: 15
plan: 05
subsystem: server
tags: [home-page, upload-ux, two-phase-fetch, conflict-list, force-overwrite, d-03, d-07, d-08]

requires:
  - phase: 15 plan 04
    provides: 409 conflict JSON contract {status,vault,conflicts[path,server_etag,changes[{ts,kind,actor}]} + force=1 re-submit semantics the JS consumes
  - phase: 14
    provides: /vaults/upload multipart pipeline, top-dir strip, locked-account gates
provides:
  - "home page 'Load local' button (D-08): folder picker opens directly, name derived from picked folder, no name input, no 'Upload vault' button"
  - "two-phase fetch upload JS (D-03): phase-1 POST -> 409 renders rich conflict list with per-file /vault/{slug}/stats wiki-changes links (D-07) -> 'Upload anyway' re-sends the SAME in-memory FileList with force=1, no re-picking"
  - "locked-account popup preserved for the new button via MORINET_LOCKED_MSG flag (server gates remain the boundary)"
affects: []

tech-stack:
  added: []
  patterns:
    - "raw-string JS const (pitfall #25) interpolated into the home-page format! slot; ES5 dialect, DOMContentLoaded-guarded"
    - "conflict rows built with createElement + textContent only — XSS-safe against attacker-controlled paths/actors; existing panel/table/empty classes only (Phase 9b: no new CSS)"

key-files:
  created: []
  modified:
    - src/server.rs

key-decisions:
  - "success redirect uses res.url (fetch follows the 303) instead of a client-side slugify — the plan allowed 'derived name's slug or the response URL'; response URL is the ground truth for both new-vault and update paths"
  - "locked UX kept as a popup: LOCKED_ACCOUNT_JS sets window.MORINET_LOCKED_MSG, LOCAL_UPLOAD_JS checks it before input.click() — the old form-interception target no longer exists, server is_locked gate unchanged (T-12-01)"
  - "LOCAL_UPLOAD_COUNT_JS retired; picked-count display folded into LOCAL_UPLOAD_JS change handler ('N files selected (folder)')"

requirements_completed: [CONF-04]
requirements-completed: [CONF-04]

one_liner: "Home page gains the D-08 'Load local' button and the D-03 two-phase fetch upload: stale re-upload renders the rich per-file conflict list with wiki-changes links and an 'Upload anyway' force re-submit of the same in-memory FileList — XSS-safe via textContent-only rendering (CONF-04 UX half)"

duration: ~30min
completed: 2026-10-30
---

# Phase 15 Plan 05: load-local-two-phase-upload-ux Summary

**CONF-04 user-facing half: the "Load local" two-phase upload UX over the 15-04 conflict contract.**

## Performance

- **Duration:** ~30 min
- **Tasks:** 2 (tests written first, RED observed)
- **Files modified:** 1 (`src/server.rs`)

## Accomplishments

- **D-08 layout** — the Phase 14 local upload form (name input + folder input + "Upload vault" button) is replaced by a `Load local` button (`id="load-local-btn"`) beside the "Load vault" control plus a hidden `<input id="local-files" type="file" webkitdirectory multiple hidden>` and an `#upload-status` container. Clicking the button opens the folder picker directly; the vault name is derived client-side from `files[0].webkitRelativePath.split('/')[0]` — no name input.
- **Two-phase fetch flow (D-03)** — new `LOCAL_UPLOAD_JS` raw-string const (ES5 dialect, DOMContentLoaded-guarded, pitfall #25 respected): phase 1 builds FormData (`name` first, then files appended with their `webkitRelativePath` as filename so the server's top-dir strip sees the native-form layout) and `fetch`-POSTs `/vaults/upload`. On 409 the JSON is parsed and rendered into the panel: heading "N files changed on the server", one table row per conflict — path linking to `/vault/{slug}/stats` (slug from `data.vault`, D-07 wiki-changes section) plus who/when lines from `changes` (`actor kind ts`) — and an `Upload anyway` button. Phase 2 re-sends the SAME in-memory FileList with `name`, `force=1`, same files; success navigates to `res.url` (the followed 303 target, `/vault/{slug}/`). Network/5xx errors render a short message in the panel.
- **XSS discipline** — every dynamic value (paths, actor emails) goes through `createElement` + `textContent`; no innerHTML interpolation of conflict data anywhere (the only remaining `innerHTML = ""` is a pre-existing unrelated clear at :1998).
- **Locked-account behavior preserved** — `LOCKED_ACCOUNT_JS` now also sets `window.MORINET_LOCKED_MSG`; the Load-local click handler shows the same popup before opening the picker. The server-side `is_locked` gate remains the security boundary (T-12-01). The locked home-page test (`home_page_shows_locked_popup_script`) passes unchanged.
- **Plain upload flow untouched (CONF-05)** — server handler not modified at all this plan; curl/plain-multipart fallback still works (15-04 contract).
- 2 new tests + 1 updated: `home_page_has_load_local_button` (button + hidden picker present; `id="up-name"` and `>Upload vault<` gone), `home_page_embeds_two_phase_upload_js` (`Upload anyway`, `force`, `webkitRelativePath`, `/vaults/upload` markers embedded).

## Task Commits

1. **Tasks 1+2: home-panel Load-local button + two-phase JS + render tests** — `823a37c` (feat, tests + markup/JS in one commit; tests written first, RED observed before the implementation)

**Plan metadata:** this commit (docs(15-05): complete plan)

## TDD Evidence

- **RED:** with the two new tests written first, `cargo test --lib home_page` → 2 passed / 2 failed (`home_page_has_load_local_button` failed on "Load local button present"; `home_page_embeds_two_phase_upload_js` failed on missing markers).
- **GREEN:** after the panel + JS implementation, `cargo test --lib home_page` → 4 passed / 0 failed; `cargo test --lib index` → 8 passed / 0 failed.

## Acceptance Criteria (all PASS)

- `grep -n 'LOCAL_UPLOAD_JS' src/server.rs` → const at :2096, interpolated into the home page at :618 ✓
- `grep -n 'Load local'` / `grep -n 'Upload anyway'` match ✓
- Home page HTML no longer contains the old form's `Upload vault` submit or `up-name` input (asserted negatively in the new test) ✓
- JS const contains `webkitdirectory` trigger, `force`, `webkitRelativePath`; conflict data rendered via `textContent` only, no `innerHTML +=` with conflict data ✓
- `cargo test --lib home_page` green (both new tests); all pre-existing home-page/index tests pass ✓
- `cargo fmt --check` clean; `cargo clippy --all-targets` no warnings ✓
- Full `cargo test`: **359 passed, 0 failed** (baseline 357 + 2 new) ✓
- `./scripts/check.sh` (fmt + clippy -D warnings + tests + release build): **OK** ✓
- No new CSS rules (only `src/server.rs` touched; no stylesheet in the diff) ✓

## Files Created/Modified

- `src/server.rs` — `index()`: upload form replaced by Load-local button + hidden picker + `#upload-status`; `LOCAL_UPLOAD_COUNT_JS` retired, new `LOCAL_UPLOAD_JS` const slotted at the same interpolation point; `LOCKED_ACCOUNT_JS` gains the `MORINET_LOCKED_MSG` flag; 2 new tests; `index_has_local_upload_form` updated

## Decisions Made

- Success redirect uses `res.url` (fetch follows the 303 to `/vault/{slug}/`) instead of a client-side slugify — no duplicated slug logic in JS, works identically for new-vault and update paths.
- Locked popup for the new button implemented via a shared `window.MORINET_LOCKED_MSG` flag rather than click interception (listener-order-proof, reuses the exact Phase 12 message string).

## Deviations from Plan

1. **`index_has_local_upload_form` updated (not "unchanged")** — the plan's acceptance said pre-existing index tests pass unchanged, but this test asserted the Phase 14 multipart form (`action="/vaults/upload"`, `enctype`, `name="files"`) that D-08 explicitly removes. Updated to assert the new layout (Load-local button, hidden picker, count span). Same precedent as 15-04's `upload_unique_slug_conflict` update.
2. **`LOCKED_ACCOUNT_JS` gained one line** (`window.MORINET_LOCKED_MSG = MSG;`) — not in the plan's file list wording, but required to keep the "existing locked-popup interception keeps working" contract once the upload form it intercepted is gone. The old `form[action="/vaults/upload"]` interception line stays (harmless no-op, keeps the locked-page test string intact).

## Issues Encountered

- None blocking. The linter noise on every `patch` call (Rust 2015 edition false positives on the whole file) is pre-existing tooling noise, not introduced here.

## Next Phase Readiness

- Phase 15 plans 01–05 all complete; ROADMAP criterion 5 user-facing half delivered (CONF-04). No blockers.
- Manual browser check (folder picker + forced modified rows on stats) remains for the verifier pass — the automated contract is fully covered by 15-04 e2e tests + this plan's render tests.

## Self-Check: PASSED

---
*Phase: MORI-15-etag-conflict-conflict-safe-co-editing-conf-01-05*
*Completed: 2026-10-30*
