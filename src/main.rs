use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

#[cfg(all(feature = "wayland", not(target_os = "linux")))]
compile_error!("The `wayland` feature is only supported on Linux.");

#[cfg(all(feature = "jack", not(target_os = "linux")))]
compile_error!("The `jack` feature is only supported on Linux.");

pub const PROJECT_NAME: &str = "transcribe-pal";

mod audio;
mod commands;
mod format;
mod lockfile;
mod model;
mod output;
mod shutdown;
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

    /// Manage models
    Model(commands::model::ModelCommandArgs),
}

fn main() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,transcribe_pal=info"));

    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_thread_ids(true)
        .with_thread_names(true)
        .init();

    // Check manifests are correct.
    if let Err(e) = model::load_manifests() {
        eprintln!("error: {e}.");
        std::process::exit(1);
    }

    let cli = Cli::parse();
    let command = cli.command.expect("Command must exist.");

    let cmd_result = match command {
        Commands::Transcribe(cmd_args) => commands::transcribe::run(cmd_args),
        Commands::Enumerate(cmd_args) => commands::enumerate::run(cmd_args),
        Commands::Model(cmd_args) => commands::model::run(cmd_args),
    };

    if let Err(err) = cmd_result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
