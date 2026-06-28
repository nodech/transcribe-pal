use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::{
    format::{SizeBase, format_disk_size, print_format_table},
    model::{self, StoreDirectoryPath},
};

mod download;

#[derive(Debug, Args)]
pub(crate) struct ModelCommandArgs {
    #[command(subcommand)]
    model_command: ModelCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ModelCommand {
    /// List models
    List(ListCommandArgs),
    /// Download model
    Download(download::DownloadCommandArgs),
    // /// Verify model files
    // Verify(VerifyCommandArgs),
}

#[derive(Debug, Args)]
pub(crate) struct VerifyCommandArgs {
    model_name: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListCommandArgs {
    /// Model store directory
    #[arg(long)]
    store_dir: Option<PathBuf>,
}

pub(crate) fn run(cmd_args: ModelCommandArgs) -> anyhow::Result<()> {
    match cmd_args.model_command {
        ModelCommand::List(args) => list_models(args)?,
        ModelCommand::Download(args) => download::download_model(args)?,
    }

    Ok(())
}

fn list_models(args: ListCommandArgs) -> anyhow::Result<()> {
    let store_dir = StoreDirectoryPath::from_opt_path(args.store_dir)?;

    let model_manifests = model::load_manifests()?;

    let headers = vec![
        "model".to_string(),
        "model size".to_string(),
        "on disk".to_string(),
        "license".to_string(),
        "homepage".to_string(),
    ];

    let mut table = vec![headers];

    // TODO: List installed thingies for filtering.

    table.extend(model_manifests.into_values().map(|f| {
        let size_on_disk = f.size_on_disk();
        let (size, unit) =
            format_disk_size(size_on_disk as f64, SizeBase::Base2);

        vec![
            f.name.to_string(),
            format!("{size} {unit}"),
            f.license_name,
            f.homepage_url.to_string(),
        ]
    }));

    print_format_table(&table, 2);
    Ok(())
}
