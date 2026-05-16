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
    Transcribe(commands::transcribe::TranscribeCommandArgs),

    /// List all available hosts and devices on the system
    Enumerate(commands::enumerate::EnumerateCommandArgs),
}

fn main() {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let cli = Cli::parse();
    let command = cli.command.expect("Command must exist.");

    let cmd_result = match command {
        Commands::Transcribe(cmd_args) => commands::transcribe::run(cmd_args),
        Commands::Enumerate(cmd_args) => commands::enumerate::run(cmd_args),
    };

    if let Err(err) = cmd_result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
