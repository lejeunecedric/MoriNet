# Plan 11-03 Summary: release v1.0.0

**Executed:** 2026-10-25 · **Release:** https://github.com/lejeunecedric/MoriNet/releases/tag/v1.0.0 (id 423655496)

## What happened

1. `scripts/release.sh` fixed first: bumps the `src/lib.rs` version guard atomically with Cargo.toml (the pushed release commit must keep the guard test green) + tolerates a no-op bump. Repo-local git identity configured (`MoriNet bot`).
2. Gate `scripts/check.sh` green → bump 1.0.0 (commit `ae77131`, pushed) → tag `v1.0.0` pushed.
3. GitHub release created via API (draft → assets → publish): `morinet-x86_64-unknown-linux-gnu.tar.xz` (5.3 MB, OpenSSL statically linked) + `.sha256` — both 201, publish 200.
4. Deployed to demo via `scripts/deploy.sh` (staging + atomic mv + restart): server reports `morinet 1.0.0`, `/health` "ok".

## Live verification

- `/mcp` without Bearer → 401 ✓
- Docs vault seeded on the demo at first 1.0.0 boot: `/var/lib/morinet/docs/morinet-docs` present ✓
- Logged-in index (demo account): three sections render, "MoriNet Documentation" under "Shared with me", Share/Statistics buttons present ✓

## Deviations / friction log

- Fine-grained PAT `morinet-release` was created Contents=Read-only; release API 403'd until the user flipped it to Read and write (Update token). Lesson: verify token write access with a probe before running release.sh.
- release.sh initially committed without git identity (repo had none) — fixed by `git config user.name/email` locally + the script fix above.
- `git push --follow-tags` does not push lightweight tags — tag was pushed explicitly. Consider `git tag -a` in release.sh later.

## Announcement

Draft ready at `ANNOUNCEMENT.md` (plan 11-02) — user posts it themselves.
