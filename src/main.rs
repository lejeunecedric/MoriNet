//! MoriNet — CLI entry point.
//!
//! Phase 1 supports `morinet serve --config <path> [--vault <book-dir>]` to run the
//! HTTP server, plus `morinet init` and `morinet version`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use morinet::{default_config, version};

#[derive(Parser, Debug)]
#[command(
    name = "morinet",
    version,
    about = "mdBook server that exposes an Obsidian second brain with auth, WebDAV and MCP",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the MoriNet version
    Version,
    /// Generate an example config.toml
    Init {
        /// Output path for the example config (defaults to ./config.toml)
        #[arg(long, default_value = "./config.toml")]
        out: PathBuf,
        /// Overwrite existing file instead of refusing
        #[arg(long)]
        force: bool,
    },
    /// Start the MoriNet HTTP server
    Serve {
        /// Path to the config.toml
        #[arg(long, default_value = "./config.toml")]
        config: PathBuf,
        /// Directory of a compiled mdbook (`book/`) to serve at /vault/
        #[arg(long)]
        vault: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Version) => {
            println!("morinet {}", version());
        }
        Some(Command::Init { out, force }) => {
            if out.exists() && !force {
                anyhow::bail!(
                    "{} already exists; pass --force to overwrite",
                    out.display()
                );
            }
            let cfg = default_config();
            std::fs::write(&out, cfg)?;
            println!("Wrote example config to {}", out.display());
        }
        Some(Command::Serve { config, vault }) => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(serve(config, vault))?,
        None => {
            println!("morinet {} — run `morinet --help` for usage", version());
        }
    }

    Ok(())
}

async fn serve(config_path: PathBuf, vault: Option<PathBuf>) -> anyhow::Result<()> {
    let file_cfg = morinet::config::load_config(&config_path)?;
    let rt = morinet::config::resolve(file_cfg.clone(), vault.clone())?;
    let bind_addr = rt.bind_addr.clone();

    // Phase 2: SQLite store, bootstrap admin.
    let data_dir = std::path::PathBuf::from(&file_cfg.storage.data_dir);
    let store = morinet::store::Store::connect(&data_dir.join("morinet.db")).await?;
    store
        .ensure_admin(&rt.admin_email, &rt.admin_password)
        .await?;
    // Plan groups + default memberships (REQ-GRP-006/007): `free` (quota 2) /
    // `admin` (unlimited), every account in its default group at boot.
    store.ensure_builtin_groups().await?;
    store.ensure_default_memberships().await?;
    // Anti-lockout: if the instance has no admin left (e.g. the only admin was
    // demoted), create/promote the fallback `admin`/`admin` account.
    store.ensure_fallback_admin().await?;
    // REQ-VLT-011: 90-day retention sweep on the access log at every boot.
    store.prune_access_log().await?;

    // Phase 3: resolve vault sources (local | github), sync + build + seed.
    use morinet::sources::{GitSource, LocalSource, VaultSource};
    let mut github_sources: Vec<(String, Box<dyn VaultSource>, String)> = Vec::new();

    if let Some(book_dir) = &vault {
        let src = LocalSource {
            path: book_dir.clone(),
        };
        let book = src.sync().and_then(|d| src.build(&d, "Demo vault"))?;
        store.seed_vault("demo", "Demo vault", &book).await?;
    } else {
        for entry in &file_cfg.vaults.entries {
            let result: anyhow::Result<PathBuf> = match entry.kind.as_str() {
                "local" => {
                    let src = LocalSource {
                        path: PathBuf::from(&entry.path),
                    };
                    src.sync().and_then(|d| src.build(&d, &entry.name))
                }
                "github" => {
                    let dest = data_dir.join("repos").join(&entry.slug);
                    let src = GitSource::github(entry.url.clone(), entry.token.clone(), dest);
                    src.sync().and_then(|d| src.build(&d, &entry.name))
                }
                other => anyhow::bail!("unknown vault kind '{other}' for '{}'", entry.slug),
            };
            match result {
                Ok(book_dir) => {
                    store
                        .seed_vault(&entry.slug, &entry.name, &book_dir)
                        .await?;
                    if entry.kind == "github" {
                        let dest = data_dir.join("repos").join(&entry.slug);
                        github_sources.push((
                            entry.slug.clone(),
                            Box::new(GitSource::github(
                                entry.url.clone(),
                                entry.token.clone(),
                                dest,
                            )),
                            entry.name.clone(),
                        ));
                    }
                }
                Err(e) => {
                    eprintln!("warning: vault '{}' failed to sync/build: {e}", entry.slug);
                }
            }
        }
    }

    // Phase 3: background periodic sync for github sources (default 300s).
    if !github_sources.is_empty() {
        let interval = file_cfg.vaults.sync_interval_secs.unwrap_or(300).max(10);
        let bg_store = store.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                for (slug, source, name) in &github_sources {
                    match source.sync().and_then(|d| source.build(&d, name)) {
                        Ok(book_dir) => {
                            if let Err(e) = bg_store.seed_vault(slug, name, &book_dir).await {
                                eprintln!("background sync: seed '{}' failed: {e}", slug);
                            }
                        }
                        Err(e) => eprintln!("background sync '{}' failed: {e}", slug),
                    }
                }
            }
        });
    }

    let app = morinet::server::build_router(rt, store);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    println!("morinet {} listening on http://{bind_addr}", version());
    axum::serve(listener, app).await?;
    Ok(())
}
