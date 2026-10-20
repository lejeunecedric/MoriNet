//! MoriNet — CLI entry point.
//!
//! Phase 0 supports `morinet --version` (default behavior) and `morinet init --out <path>`
//! to write an example `config.toml`. Phase 1+ will add `morinet serve --config <path>`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use morinet::{default_config, version};

#[derive(Parser, Debug)]
#[command(
    name = "morinet",
    version,
    about = "Serveur mdbook pour second brain Obsidian avec auth, WebDAV et MCP",
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
        None => {
            println!(
                "morinet {} — run `morinet --help` for usage",
                version()
            );
        }
    }

    Ok(())
}
