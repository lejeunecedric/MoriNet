# 12-01 Summary — locked-account demo guard (REQ-DEMO-001, REQ-DEMO-002)

**Status:** complete. Full gate `./scripts/check.sh` exits 0 (fmt + clippy -D
warnings + tests + release build). Suite: **288 passed, 0 failed, 4 ignored**
(276 baseline + 12 new).

**Commits:** `f5711bd` feat(12-01): locked-accounts config + server-side
refusal gate · `eae5942` feat(12-01): home-page locked popup script ·
this summary.

## What was built

### Task 1 — config + server gate (D-01, D-02, D-03)
- `src/config.rs`: `ENV_LOCKED_ACCOUNTS = "MORINET_LOCKED_ACCOUNTS"`;
  `SecuritySection { locked_accounts: Vec<String> }` as
  `FileConfig.security` (`#[serde(default)]`, mirrors `AdminSection`);
  `RuntimeConfig.locked_accounts`; `resolve` env-wins precedence — env split
  on `,`, entries trimmed + lowercased + empties dropped via
  `normalize_locked_list`, `.or_else` file-section fallback, never a hard
  error. `test_config()` in server.rs updated with the new field.
- `src/server.rs`: `async fn is_locked(state: &SharedState, email: &str) ->
  bool` — empty list short-circuit, admin exemption via `role_for`,
  `eq_ignore_ascii_case` compare (T-12-02). Early-return guards
  (`error_page("demo account can't add or suppress vaults")` → 400) at the
  five mutation sites: `load_personal_vault` (first statement — no network on
  refusal, T-12-05), `delete_my_vault`, `vault_edit_submit` (after
  `can_edit_vault`), `vault_share_add`, `vault_share_revoke` (after
  `can_share`). Reads/Refresh/share-view untouched.

### Task 2 — home-page flag + popup (D-04, D-05)
- `index` computes `is_locked(&state, &claims.email).await` and interpolates
  the raw-string const `LOCKED_ACCOUNT_JS` (pitfall #25 respected) in the
  AUTOFILL slot area; renders `""` for everyone else — non-locked pages stay
  byte-identical (slot placed on the `{autofill}` line so an empty render
  adds no newline).
- Script: on DOMContentLoaded blocks `form[action="/vaults/create"]` and
  every `form[action$="/delete"]` with
  `alert("demo account can't add or suppress vaults")` (exact string,
  apostrophe included) via `preventDefault` + `return false`.

## Tests added (12, all TDD-red first)
- config: `resolve_locked_accounts_env_overrides_file_section`,
  `resolve_locked_accounts_defaults_empty`,
  `resolve_locked_accounts_file_section_fallback` (extra — see Deviations).
- server routes: `locked_account_create_refused`,
  `locked_account_delete_refused`, `locked_account_edit_submit_refused`,
  `locked_account_share_add_refused`, `locked_account_share_revoke_refused`,
  `locked_account_refresh_still_allowed`, `locked_admin_email_not_locked`.
- home page: `home_page_shows_locked_popup_script`,
  `home_page_clean_for_unlocked_user`.
- Fixtures: locked user `demo@example.com` / `test-pass-9f3a`, per-test slugs
  + unique temp book dirs (pitfalls #16/#23).

## Deviations
1. **W-4 order fix (intentional, correctness):** the plan's checker snippet
   assigns `form.onsubmit = fn` and *then* calls
   `form.removeAttribute("onsubmit")`. Per the HTML event-handler spec (and
   real browser behavior) removing the content attribute unregisters the
   handler slot — including one set via the property — so the plan's order
   would silently kill the alert. Implemented order: `removeAttribute` first,
   then property assignment. Same intent (strip confirm, install popup),
   correct order.
2. **Extra config test** `resolve_locked_accounts_file_section_fallback`
   covers the `.or_else` file-section branch the two named tests don't reach.
3. **`is_locked` takes `&SharedState`** (not `&AppState`) — matches
   `can_edit_vault` and every call site; patterns §8 allowed either.
4. **Popup blocks the Load form via the `onsubmit` property** too (plan
   suggested `addEventListener` for it) — one uniform `block(form)` helper;
   behaviorally identical for the interception requirement.
5. **Refresh test proof:** the seeded vault has no upstream, so
   `refresh_my_vault` answers its own "no upstream" error page; the test
   asserts the lock string is absent — proving the gate passed the request
   through without any network access.

## Decisions
- Kept the refusal message identical to the popup string (single
  `LOCK_MSG`-equivalent literal) so tests assert one exact string on both
  surfaces (T-12-04: static, html-escaped by `error_page`).
- Split the work into two commits by surgically reverting the Task-2 hunks,
  verifying the Task-1-only tree compiles and passes `cargo test --lib
  locked` green, then restoring — each commit is independently green.
- `locked_accounts` empty short-circuit in `is_locked` avoids a `role_for`
  query on every mutation for the (default) unguarded instance.

## Lessons
- The patch-tool lint runner parses `src/server.rs` as Rust 2015 and floods
  context with false E0670 errors — ignore it; `cargo clippy` is the truth.
- `form.onsubmit = fn; form.removeAttribute("onsubmit")` is a real browser
  footgun (attribute removal nulls the property handler). Order matters.
- When a plan mandates "byte-identical for non-locked users", an interpolated
  `{slot}` must sit on an existing line — a slot on its own line adds a
  newline even when empty.
- Splitting one file's work across per-task commits: save the full file to
  scratch, string-cut the later task's blocks, commit, restore — cheaper and
  safer than interactive `git add -p` on a 6k-line file.
