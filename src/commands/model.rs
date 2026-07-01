use clap::{Args, Subcommand};

mod download;
mod list;
mod remove;

#[derive(Debug, Args)]
pub(crate) struct ModelCommandArgs {
    #[command(subcommand)]
    model_command: ModelCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ModelCommand {
    /// List models
    List(list::ListCommandArgs),
    /// Download and verify model
    Download(download::DownloadCommandArgs),
    /// Remove installed models
    Remove(remove::RemoveCommandArgs),
}

#[derive(Debug, Args)]
pub(crate) struct VerifyCommandArgs {
    model_name: String,
}

pub(crate) fn run(cmd_args: ModelCommandArgs) -> anyhow::Result<()> {
    match cmd_args.model_command {
        ModelCommand::List(args) => list::list_models(args)?,
        ModelCommand::Download(args) => download::download_model(args)?,
        ModelCommand::Remove(args) => remove::remove_model(args)?,
    }

    Ok(())
}
