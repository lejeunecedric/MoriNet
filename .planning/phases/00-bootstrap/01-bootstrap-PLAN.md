# Phase 0 — Bootstrap PLAN

> **For Hermes**: Use the subagent-driven-development skill to execute this plan bite by bite. Each bite = a fresh `delegate_task` with full context + a 2-stage review (spec compliance, then code quality).

**Goal**: lay the foundations — the repo exists on GitHub, the `morinet` crate compiles and passes its tests, the GitHub Actions CI is green on push, and `cargo install morinet` produces a binary that answers `--version`.

**Architecture**: single Rust binary crate (`src/main.rs`, `src/lib.rs`). Minimal Phase 0 dependencies: `clap`, `tokio`, `anyhow`, `serde`, `toml`. No web/auth/db in Phase 0.

**Tech stack (Phase 0)**: Rust stable (≥ 1.75), clap, tokio, anyhow, serde, toml. CI = cargo + clippy + fmt. Release = cargo-dist.

---

## Conventions for this phase

- **Bite-by-bite commits** — one commit per task, conventional-commit messages (`chore:`, `feat:`, `test:`, `ci:`, `docs:`, `build:`)
- **Strict TDD**: write the test first, watch it FAIL, write the minimal code to pass, watch it PASS
- **Absolute paths** from the repo root (e.g. `src/main.rs`, not `morinet/src/main.rs`)
- **No unnecessary deps** in Phase 0 — we add what we need, when we need it (YAGNI)

## Prerequisites before T0.1

- [ ] **GitHub username confirmed** (placeholder `lejeunecedric` substituted globally)
- [ ] `gh` CLI authed (`gh auth status` → logged in)
- [ ] Account able to create a public repo under that username
- [ ] Cargo installed locally (`cargo --version` ≥ 1.75)

---

## Task T0.1 — Init GitHub repo + local clone

**Files**:
- Create: `projets/MoriNet/` (via `gh repo create` + clone)

**Step 1: Create the repo on GitHub**

```bash
cd projets
gh repo create lejeunecedric/MoriNet --public --description "mdBook server for an Obsidian second brain, with auth, WebDAV and MCP" --source .
```

> If `--source .` is not possible (directory not init), fallback: create the empty repo via the API, then `git init` locally + `git remote add` + initial push.

**Step 2: Empty initial commit (README placeholder) then push**

```bash
cd projets/MoriNet
echo "# MoriNet" > README.md
git add README.md
git commit -m "chore: initial commit"
git push -u origin main
```

**Step 3: Verify**

```bash
gh repo view lejeunecedric/MoriNet
```

Expected: public repo, README displayed.

**Commit**: `chore: bootstrap repo MoriNet on github`

---

## Task T0.2 — Cargo init + first trivial main

**Files**:
- Create: `projets/MoriNet/Cargo.toml`
- Create: `projets/MoriNet/src/main.rs`
- Create: `projets/MoriNet/src/lib.rs`
- Create: `projets/MoriNet/.gitignore`

**Step 1: Write the failing test first (TDD)**

`src/lib.rs`:
```rust
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}
```

**Step 2: Run the test**

```bash
cd projets/MoriNet
cargo test
```

Expected: `test tests::version_is_not_empty ... ok`. If `cargo test` fails because `Cargo.toml` doesn't exist yet — that's NORMAL, it's the strict TDD "fail" before scaffolding.

**Step 3: Minimal Cargo.toml**

```toml
[package]
name = "morinet"
version = "0.1.0"
edition = "2021"
description = "mdBook server that exposes an Obsidian second brain with auth, WebDAV and MCP"
license = "MIT OR Apache-2.0"
repository = "https://github.com/lejeunecedric/MoriNet"
readme = "README.md"

[dependencies]
clap = { version = "4.5", features = ["derive"] }
tokio = { version = "1.40", features = ["macros", "rt-multi-thread"] }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"

[dev-dependencies]
pretty_assertions = "1.4"
```

**Step 4: src/main.rs**

```rust
use clap::Parser;
use morinet::version;

#[derive(Parser, Debug)]
#[command(name = "morinet", version, about = "mdBook server for an Obsidian second brain")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Print the version
    Version,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Version) => println!("morinet {}", version()),
        None => println!("morinet {} — run `morinet --help` for usage", version()),
    }
    Ok(())
}
```

**Step 5: `.gitignore`**

```
/target
**/*.rs.bk
Cargo.lock.bak
data/
config.toml
*.swp
.DS_Store
```

> Note: we do NOT commit `Cargo.lock` in Phase 0 (lib future-proofing). Later, once we have a binary, we'll commit it.

**Step 6: `cargo test` → PASS**

```bash
cargo test --verbose
```

Expected: `test result: ok. 1 passed; 0 failed`.

**Step 7: `cargo run -- --version` → expected output**

```bash
cargo run -- --version
```

Expected: `morinet 0.1.0`.

**Commit**: `feat: initial cargo crate with version subcommand`

---

## Task T0.3 — `morinet init` creates an example config.toml

**Files**:
- Modify: `src/main.rs` (add `init` subcommand)
- Modify: `src/lib.rs` (add `default_config` function)
- Create: `tests/init.rs`

**Step 1: Write the failing test**

`tests/init.rs`:
```rust
use std::fs;
use morinet::default_config;

#[test]
fn default_config_is_valid_toml_and_contains_required_sections() {
    let cfg = default_config().expect("default_config returns Ok");
    assert!(cfg.contains("[server]"), "missing [server] section");
    assert!(cfg.contains("[smtp]"), "missing [smtp] section");
    assert!(cfg.contains("[storage]"), "missing [storage] section");
}

#[test]
fn init_writes_file_when_path_provided(tmp: std::path::PathBuf) {
    // fixture via env var MORINET_TEST_TMP
    let path = tmp.join("morinet.toml");
    // (test driver runs `morinet init --config path/to/config.toml`)
    // assert file exists & readable
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("bind_addr"));
}
```

(Note: the e2e test `init_writes_file_when_path_provided` may be refined later — for Phase 0 we accept just testing `default_config()` on the lib side + a content snapshot test.)

**Step 2: Run test → FAIL** (function not defined)

```bash
cargo test --test init
```

Expected: compile error `function default_config not found`.

**Step 3: Implement**

`src/lib.rs`:
```rust
pub fn default_config() -> anyhow::Result<String> {
    let cfg = r#"# morinet — example config
# Copy this file to ./config.toml and edit values

[server]
bind_addr = "127.0.0.1:8080"
base_url = "http://localhost:8080"

[storage]
data_dir = "./data"

[smtp]
host = "smtp.example.com"
port = 587
user = ""
pass = ""
from = "morinet@example.com"
starttls = true

[session]
cookie_secret = "replace-me-with-32-bytes-of-random-hex"
cookie_name = "morinet_session"

[vaults]
# Example vault pointing to a local mdbook source folder
# [[vaults.entries]]
# slug = "demo"
# name = "Demo vault"
# kind = "local"
# path = "./data/vaults/demo/src"

[admin]
# Created on first boot if env MORINET_BOOTSTRAP_ADMIN_EMAIL is set
# bootstrap_email = "admin@example.com"
# bootstrap_password = "changeme"
"#;
    Ok(cfg.to_string())
}
```

`src/main.rs` — add to the `Command` enum:
```rust
    /// Generate an example config.toml
    Init {
        #[arg(long, default_value = "./config.toml")]
        out: std::path::PathBuf,
    },
```

And in `main()`:
```rust
        Some(Command::Init { out }) => {
            let cfg = morinet::default_config()?;
            std::fs::write(&out, cfg)?;
            println!("Wrote example config to {}", out.display());
        }
```

**Step 4: `cargo test` → PASS + `cargo run -- init --out /tmp/morinet-test.toml` → file created**

```bash
cargo test
cargo run -- init --out /tmp/morinet-test.toml
cat /tmp/morinet-test.toml
```

Expected: file contains all sections.

**Commit**: `feat: add `init` subcommand to generate example config`

---

## Task T0.4 — GitHub Actions CI (lint + test + build)

**Files**:
- Create: `projets/MoriNet/.github/workflows/ci.yml`

**Content**:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.toml') }}
      - name: Format check
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings
      - name: Test
        run: cargo test --all-features
      - name: Build release
        run: cargo build --release
```

**Commit**: `ci: github actions for lint+test+build`

> No need to test manually — pushing to `main` triggers CI.

---

## Task T0.5 — Cargo-dist for multi-platform release

**Files**:
- Modify: `projets/MoriNet/.github/workflows/ci.yml` (add a `release` job OR a separate file)

**Chosen approach**: new file `.github/workflows/release.yml` to keep CI and release separate.

**Step 1: install cargo-dist locally to scaffold**

```bash
cargo install cargo-dist
```

**Step 2: init in the repo**

```bash
cd projets/MoriNet
cargo dist init --ci=github --installers=shell,homebrew,powershell --targets=x86_64-unknown-linux-gnu,aarch64-unknown-linux-gnu,x86_64-apple-darwin,aarch64-apple-darwin,x86_64-pc-windows-msvc
```

Expected: modifies `Cargo.toml` (adds `[workspace.metadata.dist]`), creates `.github/workflows/release.yml`, creates `dist-workspace.toml`.

**Step 3: adjust the generated `release.yml`** to use our `vX.Y.Z` naming convention instead of `release-plz`.

**Step 4: `cat .github/workflows/release.yml`** + manual review.

**Commit**: `ci: add cargo-dist release workflow`

---

## Task T0.6 — Minimal README

**Files**:
- Modify: `projets/MoriNet/README.md` (replace the T0.1 placeholder)

**Content**:

```markdown
# morinet

> Server [mdBook](https://rust-lang.github.io/mdBook/) that exposes an Obsidian second brain with multi-user auth, WebDAV and MCP.

## What

- 📚 Serves one or more Obsidian vaults (compiled to mdbook) over HTTP
- 🔐 Email + password auth (HttpOnly cookies)
- 👤 `admin` / `reader` roles
- 📂 Sources = local folder OR GitHub repo
- 📡 Vaults accessible via WebDAV (same creds)
- 🤖 Vaults accessible via MCP (Model Context Protocol)
- 📨 Invitations and password reset via email

## Install

```bash
cargo install morinet
```

## Quick start

```bash
morinet init --out ./config.toml
$EDITOR ./config.toml
morinet serve --config ./config.toml
# open http://localhost:8080
```

## Status

🚧 **Phase 0 — bootstrap**. See `.planning/ROADMAP.md` for the full roadmap.

## License

MIT OR Apache-2.0
```

**Commit**: `docs: minimal readme (what/install/quick start/status/license)`

---

## Task T0.7 — First tag + release v0.1.0

**Step 1: final commit, push, then tag**

```bash
cd projets/MoriNet
git log --oneline -10  # sanity check
git push origin main
git tag v0.1.0
git push origin v0.1.0
```

**Step 2: trigger the release workflow via the tag**

The `release.yml` workflow triggers on `v*` tags. Check on GitHub Actions.

**Step 3: check release**

```bash
gh release view v0.1.0
```

Expected: release created with x86_64/aarch64 binaries for Linux/macOS/Windows.

**Commit**: *(no separate commit — the tag is enough)*

---

## Phase 0 success criteria (final verification)

```bash
cd projets/MoriNet
cargo install --path .
morinet --version        # → morinet 0.1.0
morinet init --out /tmp/c.toml  # → writes the file
gh repo view lejeunecedric/MoriNet  # → public, README present
gh release list                  # → v0.1.0 listed
```

If all these checks pass → Phase 0 ✅, we can attack Phase 1.

---

## Bite-sized notes for the subagent

- Each task = a single `delegate_task` with this PLAN in context
- 2-stage review: (1) spec compliance = is everything marked "Step" present? , (2) code quality = no clippy warnings, tests green, conventions respected
- If a task fails the 2nd review → retry once, then escalate to the user
- Commit frequency = bite-by-bite, never a mega-commit at the end of a task
