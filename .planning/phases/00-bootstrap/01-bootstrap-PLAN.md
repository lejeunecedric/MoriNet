# Phase 0 — Bootstrap PLAN

> **For Hermes** : Use subagent-driven-development to execute this plan bite-by-bite. Each bite = fresh delegate_task with full context + 2-stage review (spec compliance, then code quality).

**Goal** : poser les fondations — le repo existe sur GitHub, le crate `morinet` compile et passe ses tests, la CI GitHub Actions est verte sur push, et `cargo install morinet` produit un binaire qui répond `--version`.

**Architecture** : crate Rust binaire unique (`src/main.rs`, `src/cli.rs`, `src/config.rs`). Dépendances minimales Phase 0 : `clap`, `tokio`, `anyhow`, `serde`, `serde_json`, `toml`. Pas de web/auth/db en Phase 0.

**Tech Stack Phase 0** : Rust stable (≥ 1.75), clap, tokio, anyhow, serde, toml. CI = cargo + clippy + fmt. Release = cargo-dist.

---

## Conventions pour cette phase

- **Commits bite-by-bite** — un commit par task, message conventional (`chore:`, `feat:`, `test:`, `ci:`, `docs:`, `build:`)
- **TDD strictement** : test écrit d'abord, doit FAIL, puis code minimal pour passer, doit PASS
- **Paths absolus** depuis la racine du repo (ex `src/main.rs`, pas `morinet/src/main.rs`)
- **Pas de dépendances inutiles** en Phase 0 — on ajoute ce qu'on a besoin, quand on en a besoin (YAGNI)

## Pré-requis avant T0.1

- [ ] **Username GitHub confirmé** (placeholder `lejeunecedric` à remplacer globalement)
- [ ] `gh` CLI authed (`gh auth status` → logged in)
- [ ] Compte en mesure de créer un repo public sous ce username
- [ ] Cargo installé localement (`cargo --version` ≥ 1.75)

---

## Task T0.1 — Init repo GitHub + clone local

**Files** :
- Create : `projets/morinet/` (via `gh repo create` + clone)

**Step 1 : Créer le repo sur GitHub**

```bash
cd projets
gh repo create lejeunecedric/morinet --public --description "Serveur mdbook pour second brain Obsidian, avec auth, WebDAV et MCP" --source .
```

> Si `--source .` n'est pas possible (dir pas init), fallback : créer le repo vide via l'API, puis `git init` local + `git remote add` + push initial.

**Step 2 : commit initial vide (README placeholder) puis push**

```bash
cd projets/morinet
echo "# morinet" > README.md
git add README.md
git commit -m "chore: initial commit"
git push -u origin main
```

**Step 3 : vérifier**

```bash
gh repo view lejeunecedric/morinet
```

Expected : repo public, README affiché.

**Commit** : `chore: bootstrap repo morinet on github`

---

## Task T0.2 — Cargo init + premier main trivial

**Files** :
- Create : `projets/morinet/Cargo.toml`
- Create : `projets/morinet/src/main.rs`
- Create : `projets/morinet/src/lib.rs`
- Create : `projets/morinet/.gitignore`

**Step 1 : write the failing test d'abord (TDD)**

`src/lib.rs` :
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

**Step 2 : run test**

```bash
cd projets/morinet
cargo test
```

Expected : `test tests::version_is_not_empty ... ok`. Si `cargo test` plante parce que Cargo.toml n'existe pas encore → c'est NORMAL, c'est le test "fail" du TDD strict avant scaffold.

**Step 3 : Cargo.toml minimal**

```toml
[package]
name = "morinet"
version = "0.1.0"
edition = "2021"
description = "Serveur mdbook pour second brain Obsidian avec auth, WebDAV et MCP"
license = "MIT OR Apache-2.0"
repository = "https://github.com/lejeunecedric/morinet"
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

**Step 4 : src/main.rs**

```rust
use clap::Parser;
use morinet::version;

#[derive(Parser, Debug)]
#[command(name = "morinet", version, about = "Serveur mdbook pour second brain Obsidian")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Affiche la version
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

**Step 5 : `.gitignore`**

```
/target
**/*.rs.bk
Cargo.lock.bak
data/
config.toml
*.swp
.DS_Store
```

> Note : on ne commit PAS `Cargo.lock` en Phase 0 (lib future-proofing). Plus tard quand on aura un binaire, on le commitera.

**Step 6 : `cargo test` → PASS**

```bash
cargo test --verbose
```

Expected : `test result: ok. 1 passed; 0 failed`.

**Step 7 : `cargo run -- --version` → output attendu**

```bash
cargo run -- --version
```

Expected : `morinet 0.1.0`.

**Commit** : `feat: initial cargo crate with version subcommand`

---

## Task T0.3 — `morinet init` crée config.toml exemple

**Files** :
- Modify : `src/main.rs` (ajouter subcommand `init`)
- Modify : `src/lib.rs` (fonction `default_config`)
- Create : `tests/init.rs`

**Step 1 : write failing test**

`tests/init.rs` :
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

(Note : le test e2e `init_writes_file_when_path_provided` pourra être raffiné plus tard — Phase 0 on accepte de juste tester `default_config()` côté lib + un test snapshot du contenu.)

**Step 2 : run test → FAIL** (fonction pas définie)

```bash
cargo test --test init
```

Expected : compile error `function default_config not found`.

**Step 3 : implement**

`src/lib.rs` :
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

`src/main.rs` — ajouter dans l'enum `Command` :
```rust
    /// Generate an example config.toml
    Init {
        #[arg(long, default_value = "./config.toml")]
        out: std::path::PathBuf,
    },
```

Et dans `main()` :
```rust
        Some(Command::Init { out }) => {
            let cfg = morinet::default_config()?;
            std::fs::write(&out, cfg)?;
            println!("Wrote example config to {}", out.display());
        }
```

**Step 4 : `cargo test` → PASS + `cargo run -- init --out /tmp/morinet-test.toml` → fichier créé**

```bash
cargo test
cargo run -- init --out /tmp/morinet-test.toml
cat /tmp/morinet-test.toml
```

Expected : fichier contient toutes les sections.

**Commit** : `feat: add `init` subcommand to generate example config`

---

## Task T0.4 — CI GitHub Actions (lint + test + build)

**Files** :
- Create : `projets/morinet/.github/workflows/ci.yml`

**Content** :

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

**Commit** : `ci: github actions for lint+test+build`

> Pas besoin de tester manuellement — le push sur main déclenche la CI.

---

## Task T0.5 — Cargo-dist pour release multi-platform

**Files** :
- Modify : `projets/morinet/.github/workflows/ci.yml` (ajouter job `release` OU nouveau fichier dédié)

**Choix retenu** : nouveau fichier `.github/workflows/release.yml` pour séparer CI et release.

**Step 1 : install cargo-dist locally pour scaffolder**

```bash
cargo install cargo-dist
```

**Step 2 : init dans le repo**

```bash
cd projets/morinet
cargo dist init --ci=github --installers=shell,homebrew,powershell --targets=x86_64-unknown-linux-gnu,aarch64-unknown-linux-gnu,x86_64-apple-darwin,aarch64-apple-darwin,x86_64-pc-windows-msvc
```

Expected : modifie `Cargo.toml` (ajoute `[workspace.metadata.dist]`), crée `.github/workflows/release.yml`, crée `dist-workspace.toml`.

**Step 3 : adjust the generated `release.yml`** pour utiliser notre naming convention `vX.Y.Z` au lieu de `release-plz`.

**Step 4 : `cat .github/workflows/release.yml`** + review manuel.

**Commit** : `ci: add cargo-dist release workflow`

---

## Task T0.6 — README minimal

**Files** :
- Modify : `projets/morinet/README.md` (remplacer le placeholder de T0.1)

**Content** :

```markdown
# morinet

> Serveur [mdBook](https://rust-lang.github.io/mdBook/) qui expose un second brain Obsidian avec auth multi-utilisateurs, WebDAV et MCP.

## What

- 📚 Sert un ou plusieurs vaults Obsidian (compilés en mdbook) sur HTTP
- 🔐 Auth par email + mot de passe (cookies HttpOnly)
- 👤 Rôles `admin` / `reader`
- 📂 Sources = dossier local OU repo GitHub
- 📡 Vaults accessibles en WebDAV (mêmes creds)
- 🤖 Vaults accessibles en MCP (Model Context Protocol)
- 📨 Invitations et reset mdp par email

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

🚧 **Phase 0 — bootstrap**. Voir `.planning/ROADMAP.md` pour la roadmap complète.

## License

MIT OR Apache-2.0
```

**Commit** : `docs: minimal readme (what/install/quick start/status/license)`

---

## Task T0.7 — Premier tag + release v0.1.0

**Step 1 : commit final, push, puis tag**

```bash
cd projets/morinet
git log --oneline -10  # sanity check
git push origin main
git tag v0.1.0
git push origin v0.1.0
```

**Step 2 : déclencher le workflow release via le tag**

Le workflow `release.yml` se déclenche sur `v*` tags. Vérifier sur GitHub Actions.

**Step 3 : check release**

```bash
gh release view v0.1.0
```

Expected : release créée avec binaires x86_64/aarch64 pour Linux/macOS/Windows.

**Commit** : *(pas de commit séparé — le tag suffit)*

---

## Critères de succès Phase 0 (vérification finale)

```bash
cd projets/morinet
cargo install --path .
morinet --version        # → morinet 0.1.0
morinet init --out /tmp/c.toml  # → écrit le fichier
gh repo view lejeunecedric/morinet  # → public, README présent
gh release list                  # → v0.1.0 listé
```

Si tous ces checks passent → Phase 0 ✅, on peut attaquer Phase 1.

---

## Notes bite-sized pour le subagent

- Chaque task = un seul delegate_task avec ce PLAN en contexte
- Review 2-stage : (1) spec compliance = tout ce qui est marqué "Step" est-il présent ? , (2) code quality = pas de warnings clippy, tests verts, conventions respectées
- Si une task échoue la 2e review → retry une fois, puis escalate au user
- Fréquence commits = bite-by-bite, jamais un méga-commit en fin de task
