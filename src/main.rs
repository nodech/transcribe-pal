use std::sync::{Arc, atomic::AtomicBool};

use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

mod audio;
mod commands;
mod output;
mod transcribe;

#[derive(Parser)]
#[command(subcommand_required = true, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Transcribe system audio to text
    Transcribe {
        /// Audio host on the system
        #[arg(long)]
        host: Option<String>,
        /// Audio device on the host
        #[arg(long)]
        device: Option<String>,
    },
    /// List all available hosts and devices on the system
    Enumerate {
        #[arg(short, long)]
        debug: bool,
        // /// Model name to check the required audio configs.
        // #[arg(long)]
        // model: Option<String>
    },
}

fn main() -> Result<(), anyhow::Error> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let cli = Cli::parse();
    let command = cli.command.expect("Command must exist.");

    match command {
        Commands::Transcribe { host, device } => {
            commands::transcribe::run(host, device)
                .with_context(|| "Failed to transcribe.")?
        }
        Commands::Enumerate { debug } => commands::enumerate::run(debug)
            .with_context(|| "Failed to enumerate.")?,
    };

    Ok(())
}
