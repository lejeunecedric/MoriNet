# MoriNet — Planning (human entry point)

> GSD planning for MoriNet — a Rust server that exposes an Obsidian second brain via mdbook with multi-user auth, admin UI, vault management (local + GitHub), WebDAV, and MCP.

## TL;DR

- **Stack**: 100% Rust (`cargo install morinet` produces a static binary)
- **8 phases**: bootstrap → core-serve → multi-vault → vault-sources → admin-ui → email → webdav → mcp → polish
- **Repo**: github.com/`lejeunecedric`/MoriNet

## What to read, in order

1. **`../.planning/ROADMAP.md`** — overview of the 8 phases, success criteria, scope
2. **`../.planning/REQUIREMENTS.md`** — 48 REQ-IDs catalog with verification commands
3. **`../.planning/STATE.md`** — architectural decisions (AD-1 to AD-10) + live stats
4. **`../.planning/phases/00-bootstrap/01-bootstrap-PLAN.md`** — first bite to execute: bootstrap the repo + the crate + the CI

## Locked decisions

| #    | Decision                                       | Tracked in   |
|------|------------------------------------------------|--------------|
| PRD-1 | Stack 100% Rust                                | STATE.md     |
| PRD-2 | Repo github.com/`lejeunecedric`/MoriNet        | STATE.md     |
| PRD-3 | Multi-vault = sub-paths `/vaultA/`, `/vaultB/` | STATE.md     |
| PRD-4 | SMTP = generic `lettre` adapter                | STATE.md     |
| AD-1  | Cargo workspace mono-crate                     | STATE.md     |
| AD-2  | mdbook upstream + custom theme injected        | STATE.md     |
| AD-3  | SQLite by default                              | STATE.md     |
| AD-4  | HttpOnly signed cookies + bcrypt               | STATE.md     |
| AD-5  | Custom axum handlers for WebDAV                | STATE.md     |
| AD-6  | MCP via `rmcp` crate                           | STATE.md     |
| AD-7  | SMTP via `lettre` crate                        | STATE.md     |
| AD-8  | Vault source = trait, Local + Github impls     | STATE.md     |
| AD-9  | HTML templates via Askama                      | STATE.md     |
| AD-10 | mdbook rebuild after sync, not per request     | STATE.md     |

## Phase 0 — bootstrap (to execute)

**Goal**: GitHub repo + Cargo init + CI + first release.

**Bites**:
- T0.1 → confirm GitHub username (placeholder `lejeunecedric` to substitute)
- T0.2 → `cargo install morinet` builds a trivial binary + `cargo test` green
- T0.3 → GitHub Actions CI (lint + test + build)
- T0.4 → cargo-dist for multi-platform release
- T0.5 → minimal README (What / Install / Quick start / Status / License)
- T0.6 → first commit + push + tag `v0.1.0`

See `.planning/phases/00-bootstrap/01-bootstrap-PLAN.md` for bite-sized detail.

## Next step

Once the username is confirmed, Phase 0 is launched via subagent-driven-development (fresh `delegate_task` per bite + double review: spec compliance + code quality).

## Out of scope v1

Real-time collaborative editing, native mobile apps, OAuth SSO, webhooks, encryption at rest (relies on the FS).
