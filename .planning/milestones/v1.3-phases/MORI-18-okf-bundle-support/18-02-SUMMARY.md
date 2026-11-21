---
id: "18-02"
one_liner: "migration 0013 vaults.okf_json column + set_vault_okf/vault_okf store helpers"
requirements_completed: [OKF-01]
status: complete
commit: 7310be8
---

# 18-02 Summary — migration-0013-and-store-persistence

## What was built

`migrations/0013_okf_conformance.sql` (new): 0011-style header comment
(`-- Phase 18 (OKF-01, D-03): OKF conformance stats per vault.` + okf_json
explanation) and exactly one statement:
`ALTER TABLE vaults ADD COLUMN okf_json TEXT;` — nullable, no DEFAULT, no
CHECK; picked up automatically by `sqlx::migrate!("./migrations")`.

`src/store.rs` — two helpers immediately after `set_vault_source` (same
section, exact sqlx shapes of the vault_source pair):

```rust
/// Remember the latest OKF conformance scan (set at refresh/register;
/// best-effort callers). Opaque JSON text — serde validates in Rust (D-03).
pub async fn set_vault_okf(&self, slug: &str, json: &str) -> anyhow::Result<()>
// UPDATE vaults SET okf_json = ? WHERE slug = ?

/// The stored OKF conformance JSON (None = never scanned; unknown slug
/// collapses to the same None — no existence leak, T-18-04).
pub async fn vault_okf(&self, slug: &str) -> anyhow::Result<Option<String>>
// query_as::<(Option<String>,)> + fetch_optional + and_then
```

The store treats the JSON as opaque TEXT — no SQL-side validation (T-18-03).

New test module `store::okf_store_tests` (5 tests):
`okf_migration_adds_column` (PRAGMA table_info(vaults): okf_json present,
notnull == 0), `vault_okf_roundtrip` (byte-exact string round-trip),
`vault_okf_none_when_never_scanned`, `vault_okf_none_for_unknown_slug`
(same None shape — T-18-04), `set_vault_okf_overwrites_previous`.

## TDD evidence

- **RED** (tests written first, helpers/column absent): `cargo test --lib -- okf_migration vault_okf set_vault_okf` → 7 × E0599 `method not found` for `set_vault_okf`/`vault_okf` — "could not compile morinet (lib test) due to 7 previous errors".
- **GREEN**: same command → 5 passed / 0 failed.
- Full suite: **511 passed / 0 failed / 4 ignored** (baseline 506 + 5 new).
- `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean; `git diff --exit-code Cargo.toml Cargo.lock` clean — zero new dependencies.
- Acceptance greps: single `ALTER TABLE` in 0013 (count == 1); both helper signatures present in store.rs.

## Deviations

None. Plan followed exactly (file name, header style, single ALTER, helper
placement/signatures/doc-comments, test names).

## Decisions

- Migration-shape test checks presence + nullability of `okf_json` rather than pinning the whole `vaults` column list (that table accretes columns per phase — PATTERNS §4 — so a full-list pin would churn every phase; the 0012-style full pin only fits brand-new tables).
- Test module placed at the end of store.rs as `mod okf_store_tests`, matching the per-phase module convention (vault_changes_tests, identity_tests).

## Lessons

- `cargo test --lib a b c` (filters before `--`) errors; the working form is `cargo test --lib -- okf_migration vault_okf set_vault_okf` — libtest accepts multiple filters after `--`.
- PRAGMA table_info rows read fine via `r.get::<String, _>("name")` / `get::<i64, _>("notnull")` with `use sqlx::Row` in the test module.

## notes_for_next_executor

- **Helper signatures (18-03 refresh hook / badge, 18-05 API):**
  - `store.set_vault_okf(slug: &str, json: &str).await -> anyhow::Result<()>` — best-effort caller should log-and-ignore errors (inert write; never fails on unknown slug, UPDATE just affects 0 rows).
  - `store.vault_okf(slug: &str).await -> anyhow::Result<Option<String>>` — `None` for never-scanned AND unknown slug (no existence leak; callers must not branch on existence).
- **Stored JSON shape** = serde of `crate::okf::OkfStats` (18-01):
  `{"version":"0.2"|"0.1"|null,"concepts":<usize>,"total":<usize>,"has_index":<bool>,"has_log":<bool>,"scanned_at":<u64 epoch secs>}`.
  Writers: `serde_json::to_string(&stats)?`. Readers (badge, API): `serde_json::from_str::<crate::okf::OkfStats>(&s)` **defensively** — on Err fall back to "no badge / no okf field" (T-18-03 pin).
