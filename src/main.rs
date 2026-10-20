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
        Some(Command::Serve { config, vault }) => {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(serve(config, vault))?
        }
        None => {
            println!("morinet {} — run `morinet --help` for usage", version());
        }
    }

    Ok(())
}

async fn serve(config_path: PathBuf, vault: Option<PathBuf>) -> anyhow::Result<()> {
    let file_cfg = morinet::config::load_config(&config_path)?;
    let rt = morinet::config::resolve(file_cfg, vault)?;
    let bind_addr = rt.bind_addr.clone();
    let app = morinet::server::build_router(rt);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    println!("morinet {} listening on http://{bind_addr}", version());
    axum::serve(listener, app).await?;
    Ok(())
}
