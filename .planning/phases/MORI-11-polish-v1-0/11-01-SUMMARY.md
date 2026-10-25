# Plan 11-01 Summary: Interactive `morinet init`

**Phase:** MORI-11-polish-v1-0 · **Plan:** 01 · **Status:** Complete
**Commits:** `486b969` (Task 1), `2d7ae5c` (Task 2), `1b7a591` (Task 3)

## What was built

- **`src/init.rs`** (new module, `pub mod init;` in lib.rs):
  - `InitAnswers { port, data_dir, admin_email, admin_password }`
  - `generate_cookie_secret()` — 64 lowercase hex chars from two `Uuid::new_v4().simple()` concatenations (no new RNG dep)
  - `generate_config(port, data_dir, email, password, secret)` — commented TOML modeled on `default_config()` (same section order/comments), port substituted into `bind_addr`/`base_url`, real `[admin]` section with plaintext-bootstrap + chmod-600 comment; all user values TOML-escaped via `toml::Value::String`
  - `prompt_answers(reader, writer)` — cargo-new-style prompts `Port [8080]:` / `Data dir [./data]:` / `Admin email [admin@example.com]:`, injected `BufRead` (TTY-free testable), Enter accepts default, invalid port re-prompts, EOF falls back to defaults
- **`Cargo.toml`**: `rpassword = "7"` added (v7.5.4 locked)
- **`src/config.rs`**: `AdminSection { email: Option<String>, password: Option<String> }` + `#[serde(default)] admin` on `FileConfig`; `resolve()` precedence now env `MORINET_ADMIN_EMAIL`/`_PASSWORD` → `[admin]` file values (empty strings ignored) → existing "missing admin email/password" errors (wording preserved)
- **`src/main.rs`**: `Command::Init` gains `--yes` (writes `default_config()` verbatim, byte-identical); interactive path = `prompt_answers` → masked password via `rpassword::prompt_password` (re-prompts while empty) → `generate_config` with fresh secret; `--out`/`--force` semantics untouched (refusal runs before any prompt); success prints plaintext-credentials hint

## Tests (TDD: RED verified before each GREEN)

- `cargo test --lib`: **270 passed, 0 failed** (incl. untouched `default_config_contains_required_sections` / `default_config_parses_as_valid_toml`)
- New init tests (5): generated TOML parses + carries all values verbatim; special-char escaping round-trips; secret is 64-hex and unique; prompt defaults on `"\n\n\n"`; override + invalid-port re-prompt
- New config tests (3): `[admin]` file fallback when env absent (tempdir file); env overrides `[admin]`; absent `[admin]` still deserializes
- `cargo clippy --all-targets -- -D warnings`: clean; `cargo fmt` applied
- Manual smoke: `init --yes --out` valid TOML + byte-identical to `default_config()` (cmp vs extracted template); double `init --out` without `--force` exits 1 with "already exists; pass --force"; `init --help` lists `--out/--force/--yes`; piped interactive run (invalid port + empty password re-prompts) produced config with entered values + 64-hex secret; **E2E boot**: generated config, no env vars → `serve` listened and `POST /login` with the bootstrap admin returned 303 → `/`

## Deviations

- **Password read location**: plan sketch had `prompt_answers` take the password as a parameter; implemented as two-stage instead (prompt_answers returns empty `admin_password`, main fills it) so the password prompt comes **last** (better UX, matches acceptance-criteria prompt order). Same testability.
- **Non-TTY password fallback**: `rpassword` requires `/dev/tty` and errors (ENXIO) when stdin is piped. `interactive_init` uses `std::io::stdin().is_terminal()`: masked `rpassword` on a TTY, plain stdin line otherwise (needed for the piped smoke test; masked-input guarantee on real terminals unchanged).
- Test fixture password is `test-pass-9f3a` (repo-neutral fixture) instead of the plan example `s3cret`.

## Decisions

- TOML string values rendered via `toml::Value::String` Display → quotes/backslashes/`#` in passwords and paths can't corrupt the file (extra round-trip test added).
- `[admin]` file fallback ignores empty strings, so `email = ""` still yields the "missing admin email" error instead of an empty-account bootstrap.
- Cookie-secret generation stays UUID-based per plan (no `rand` dependency); `default_config()` left byte-for-byte untouched.

## Lessons

- `rpassword` is not pipe-safe — any CLI test harness piping stdin must branch on `IsTerminal` (saved in this summary for future CLI prompts).
- The write-file lint harness runs rustfmt in edition-2015 mode and reports false `E0670 async fn` errors on this repo; trust `cargo fmt`/`cargo build` instead.
- `/login` (not `/auth/login`) is the login route when smoke-testing auth flows.
