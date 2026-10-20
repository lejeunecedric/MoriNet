# Phase 1 — core-serve PLAN

> **For Hermes**: execute bite-by-bite, TDD strict, conventional commits. Planning doc for Phase 1 per ROADMAP.

**Goal**: `morinet serve` starts axum on `:8080`; a single hardcoded admin (env) can log in and see ONE compiled vault at `data/vaults/demo/book/`, gated by a signed HttpOnly cookie.

**Out of scope (Phase 1)**: SQLite, multi-vault, ACL tables (→ Phase 2). Askama templates (→ Phase 4 admin-ui; Phase 1 login form is inline HTML).

---

## Task T1.1 — deps + config module

**Files**: `Cargo.toml` (add axum, tower-http, tower, hmac, sha2, subtle, base64), `src/config.rs`.

- `ServerConfig` parse from `config.toml`: `[server].bind_addr`, `[server].base_url`
- `SessionConfig`: `[session].cookie_secret` (required: env `MORINET_COOKIE_SECRET` overrides config; error if neither), `cookie_name`
- `AdminConfig` from env: `MORINET_ADMIN_EMAIL`, `MORINET_ADMIN_PASSWORD` (plain for Phase 1/staging only; bcrypt+DB in Phase 2 — documented)
- Unit tests: config parse, secret resolution precedence, missing-secret error

## Task T1.2 — session module (HMAC-signed cookie)

**Files**: `src/session.rs`.

- Token format: `b64url(email).expiry_unix.b64url(hmac_sha256(secret, "email.expiry"))`
- `SessionCodec::sign(email, ttl) -> token`, `SessionCodec::verify(token) -> Option<String>`
- Constant-time compare (`subtle`), expiry check, reject on any parse failure
- Unit tests: roundtrip ok; tampered payload → None; expired → None; wrong secret → None

## Task T1.3 — axum router: health + login + auth middleware

**Files**: `src/server.rs`.

- `GET /health` → 200 "ok" (no auth)
- `GET /login` → inline HTML form
- `POST /login` (form: email, password) → valid creds → `Set-Cookie: morinet_session=...; HttpOnly; Path=/; SameSite=Lax` + redirect `/`; invalid → 401 (form re-render)
- `require_session` middleware: no/invalid cookie on protected routes → redirect `/login`
- Tests via `tower::ServiceExt::oneshot` on the Router: health 200, protected route redirects when anonymous, login roundtrip sets cookie, cookie allows access

## Task T1.4 — serve the compiled vault

**Files**: `src/server.rs`.

- `GET /` (authenticated) → redirect `/vault/demo/`
- `/vault/{*path}` → `tower_http::services::ServeDir` rooted at configured vault book dir, behind the auth middleware
- Fixture test: temp dir with a fake `book/index.html`, request with cookie → 200 contains content; without → redirect

## Task T1.5 — `morinet serve` subcommand wiring

**Files**: `src/main.rs`.

- `Command::Serve { config: PathBuf, vault: Option<PathBuf> }` → load config → build router → `tokio::net::TcpListener` + `axum::serve`
- Smoke check: `morinet serve --config /tmp/c.toml --vault /tmp/book` + `curl /health` → 200, then Ctrl-C

## Task T1.6 — CI green + commit

- `cargo fmt`, `cargo clippy -D warnings`, `cargo test` all green
- Commit bite-by-bite: `feat(session): hmac-signed cookie codec`, `feat(server): axum router with health+login`, `feat(server): gated vault serving`, `feat(cli): morinet serve subcommand`
- Push; next release (v0.1.1) auto-deploys to holldemo via deploy.yml

## Success criteria (per ROADMAP Phase 1)

- [x] `morinet serve` starts on `:8080`
- [x] `GET /` redirects to `/login` when anonymous
- [x] `POST /login` with `MORINET_ADMIN_*` creds sets signed HttpOnly cookie
- [x] `GET /vault/*` serves mdbook files, gated by the cookie
