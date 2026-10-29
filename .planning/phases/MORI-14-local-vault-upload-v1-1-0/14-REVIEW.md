---
status: issues_found
---

# Phase 14 Code Review — local vault upload (commits 618c770, 53afad4)

**Reviewed:** 2026-10-29. Scope: `git show 618c770` (upload endpoint) + `git show 53afad4` (home form/JS) against `14-01-PLAN.md` (incl. checker fixes: slug pre-check before disk, name-before-files abort) and locked decisions D-01..D-06 in `14-CONTEXT.md`. All claims verified against the live tree at HEAD (`676ca38`); `cargo test --lib` re-run: **332 passed, 0 failed, 4 ignored**.

**Verdict:** The plan is implemented faithfully — gate order, sanitizer, caps, scoped body limit, cleanup paths, and both checker fixes are present and test-covered. Two real bugs found: one HIGH (concurrency data-loss race that survives the checker's sequential pre-check) and one MEDIUM (new unvalidated-slug filesystem removal in the admin delete flow). Neither is hit by the sequential tests, which is why the suite is green.

---

## HIGH-1 — Concurrent same-slug uploads: the loser's `fail_upload` deletes the winner's live vault tree

**Where:** `src/server.rs:2984-2994` (pre-check → `remove_dir_all` → `create_dir_all`), `src/server.rs:3079-3086` (UNIQUE arm → `fail_upload` → `remove_dir_all(dir)`).

The checker MAJOR fix (slug pre-check before any disk touch) closes the **sequential** collision, but the pre-check is TOCTOU-racy:

1. Two concurrent `POST /vaults/upload` requests with the same name (trivially fired in parallel by any authenticated non-locked user; cross-user variant: attacker races a victim's in-flight upload using the same vault name).
2. Both pass the pre-check at :2984-2989 — no vault row yet, `try_exists` false for both.
3. Both `remove_dir_all` + `create_dir_all` the same `uploads/<slug>` (B's wipe also destroys A's partially written files; the two streams interleave writes into one mixed tree).
4. A finishes first, registers via `create_personal_vault` → 303. The served book lives **inside** `uploads/<slug>` (D-04).
5. B hits the UNIQUE arm at :3079 → `fail_upload(&dir, ...)` → `remove_dir_all(dir)` deletes **A's now-registered, live tree**. A's vault row points at a deleted `book_path`.

The comment at :2991-2992 ("the pre-check above guarantees no live vault owns this dir") is exactly the assumption the race breaks — the guarantee holds only sequentially. `upload_unique_slug_conflict` proves the sequential case only.

**Fix direction:** make dir creation exclusive — `tokio::fs::create_dir(&dir)` and treat `AlreadyExists` as "already taken — rename the vault" (this also subsumes the `try_exists` pre-check); and/or in the UNIQUE arm, re-check `store.vault_name(&slug)` and skip `remove_dir_all` when a row now owns the slug.

## MEDIUM-1 — `admin_delete_vault` feeds the raw path slug to `remove_dir_all`; `..` wipes the entire `data_dir`

**Where:** `src/server.rs:3365` (new cleanup line), `src/store.rs:527-533` (`delete_vault` returns `Ok(())` even when zero rows match — no rowcount check).

`admin_delete_vault` takes `Path(slug)` unvalidated. Verified with a matchit 0.8.4 probe (the router's exact version per `Cargo.lock`): `POST /admin/vaults/../delete` matches `{slug} = ".."` (matchit does not normalize literal `..` segments; hyper/axum do not either). Then `store.delete_vault("..")` is a no-op DELETE that still returns `Ok(())`, so the new D-05 cleanup runs `remove_dir_all(data_dir/uploads/..)` = **`remove_dir_all(data_dir)`** — unlinks the SQLite DB, `repos/`, and every user's `uploads/` tree (Linux happily unlinks the open DB file). The `.` variant (`/admin/vaults/./delete`) wipes the whole `uploads/` root. Before this commit the same request was a harmless no-op; this is a **new regression introduced by 618c770**.

Mitigating factors (why MEDIUM, not HIGH): the route sits behind `require_admin` (`src/server.rs:229-230`) and the session cookie is `SameSite=Lax` (`src/server.rs:408`), which blocks cross-site POST — so only an admin (or an admin-session XSS/theft) can trigger it. Still: it silently escalates "admin deletes one vault" to "admin request nukes all data".

`delete_my_vault` is **not** affected: its removal runs only on `Ok(true)`, which requires an owned DB row, so only DB-charset slugs reach the filesystem there (`src/server.rs:2068`).

**Fix direction:** guard both removals with a slug-shape check (e.g. non-empty and all `is_ascii_alphanumeric() || c == '-'`) before touching the filesystem — DB slugs always satisfy it.

## LOW-1 — Orphan `uploads/<slug>` dir permanently blocks re-upload of the same name; the orphan-cleanup `remove_dir_all` is dead code

**Where:** `src/server.rs:2985` (`try_exists` → reject) vs `src/server.rs:2993` (unreachable wipe).

Because the pre-check rejects whenever the dir exists — even with no vault row — the `remove_dir_all` at :2993 ("orphan defense only") can never run: reaching it requires `try_exists == false`. Any orphan left by a crash between `create_dir_all` and registration (or a failed `fail_upload`) makes every future upload of that name fail with "the slug '…' is already taken" until an admin manually removes the dir. Note the plan text is self-contradictory here ("try_exists → error_page" vs "proceed to wipe … if the dir exists (orphan)"); the implementation chose the stricter reading, which is safe but strands orphans. Acceptable if intentional; otherwise wipe only when `vault_name` is None AND `try_exists` is true.

## LOW-2 — Two early-error arms create the dir without cleanup

**Where:** `src/server.rs:2994-2996` (`create_dir_all` Err) and `:2997-3000` (`canonicalize` Err) return `error_page` without `fail_upload`. A partially created dir can survive (and per LOW-1 then blocks the name forever). Minor; fold into the LOW-1 decision.

## INFO — verified-correct items (checked, no action)

- **INFO-1 Caps have no off-by-one:** `file_count > MAX_FILES` (`:3022`) admits exactly 2000; `total > MAX_FILE_BYTES` (`:3045`) admits exactly 5 MiB — matches "max 2000 / max 5 MB". The size cap aborts before writing the offending chunk; count cap aborts before writing the 2001st file. `file_count` counts only written markdown parts (plan: "count file parts written"); ignored junk is bounded by the 256 MB body limit instead.
- **INFO-2 Body-limit scoping:** `DefaultBodyLimit::max(256 MB)` is a handler-scoped `.layer()` on the upload route only (`src/server.rs:185-186`), inside the protected router so it inherits `require_session` (D-01). Proven both ways by `upload_body_limit_scoped` (3 MB upload 303; >2 MB `/vaults/create` 413).
- **INFO-3 Sanitizer audit (`sanitize_upload_name`, `:2875-2915`):** empty raw → Rejected; `\`, `:`, NUL, leading `/`, `..`/`.`/empty segments → Rejected (whole-upload abort per D-03); top-level strip keeps a bare single segment per plan. URL-encoded names are a non-issue: multer 3.1.0 does **not** percent-decode `filename` (verified in the vendored crate), so `%2e%2e/x.md` stays a literal harmless dir name. Symlink writes are impossible — multipart parts become regular files, and the canonicalized-parent prefix check (`:3032-3035`) rejects any parent that resolves outside the canonicalized root. Trailing-slash names (`MyWiki/`) hit the empty-segment rule → Rejected (strict, spec-conformant).
- **INFO-4 Gate order (D-05) + checker fixes present:** locked first (`:2933`, exact LOCK_MSG) → name required + **file-before-name abort** (`:2941-2950`) → slug (`:2955`) → quota (`:2962-2978`, same messages) → slug pre-check (`:2984`) → first disk touch (`:2993`). The handler-vs-registration quota TOCTOU is backstopped by the store-level re-check inside `create_personal_vault` (`src/store.rs:917-925`).
- **INFO-5 No XSS:** the home form is static HTML (`:587-591`); no user input is reflected there. `error_page` html-escapes every interpolated message (`:3268-3279`), and interpolated slugs are `[a-z0-9-]`-only anyway. `LOCAL_UPLOAD_COUNT_JS` uses `textContent` (`:1981-1994`); the locked-popup extension reuses the existing `block()`/MSG (`:2016-2017`) — no duplication, server gate remains the boundary (D-06).
- **INFO-6 `vault_slug_from_name` extraction is behavior-identical:** `:2854-2861` is byte-for-byte the code removed from `load_personal_vault` (diff hunk at old :2775); the "the name must contain letters or digits" error is kept in both callers. No regression.
- **INFO-7 D-04 honored:** `set_vault_source` is never called from the upload path; `upload_registers_personal_vault` asserts `vault_source` is None and the built book serves 200.
- **INFO-8 Delete-flow cleanup present in both required arms:** `delete_my_vault` `Ok(true)` (`:2068`) and `admin_delete_vault` `Ok(())` (`:3365`), best-effort `let _ =` with the plan's repos/-comment; `repos/` behavior untouched as specified.

## Test-suite note

332/332 green (re-run at review time). The two bugs above are invisible to the suite because both tests are strictly sequential (`oneshot` requests one at a time). A regression test for HIGH-1 would fire two uploads concurrently (e.g. `tokio::join!`) and assert the registered vault's tree survives; for MEDIUM-1, a `POST /admin/vaults/../delete` as admin asserting `data_dir` still exists.

---
*Reviewer: Hermes code review, phase MORI-14-local-vault-upload-v1-1-0.*
