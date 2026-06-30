use clap::{Args, Subcommand};

mod download;
mod list;

#[derive(Debug, Args)]
pub(crate) struct ModelCommandArgs {
    #[command(subcommand)]
    model_command: ModelCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ModelCommand {
    /// List models
    List(list::ListCommandArgs),
    /// Download model
    Download(download::DownloadCommandArgs),
    // /// Verify model files
    // Verify(VerifyCommandArgs),
}

#[derive(Debug, Args)]
pub(crate) struct VerifyCommandArgs {
    model_name: String,
}

pub(crate) fn run(cmd_args: ModelCommandArgs) -> anyhow::Result<()> {
    match cmd_args.model_command {
        ModelCommand::List(args) => list::list_models(args)?,
        ModelCommand::Download(args) => download::download_model(args)?,
    }

    Ok(())
}
