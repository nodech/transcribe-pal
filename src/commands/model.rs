use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, Subcommand};

use crate::{
    format::{SizeBase, format_disk_size, print_format_table},
    model::{self, StoreDirectoryPath},
    transcribe::ModelKind,
};

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
    Download(DownloadCommandArgs),
    // /// Verify model files
    // Verify(VerifyCommandArgs),
}

#[derive(Debug, Args)]
pub(crate) struct VerifyCommandArgs {
    model_name: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListCommandArgs {}

#[derive(Debug, Args)]
pub(crate) struct DownloadCommandArgs {
    /// Model name
    model_name: String,

    /// Model store directory
    #[arg(long)]
    store_dir: Option<PathBuf>,

    /// Accept the license of the model.
    #[arg(long, short)]
    yes: bool,

    /// Overwrite files if necessary.
    #[arg(long, short)]
    force: bool,
}

pub(crate) fn run(cmd_args: ModelCommandArgs) -> anyhow::Result<()> {
    match cmd_args.model_command {
        ModelCommand::List(args) => list_models(args)?,
        ModelCommand::Download(args) => download_model(args)?,
    }

    Ok(())
}

fn download_model(args: DownloadCommandArgs) -> anyhow::Result<()> {
    let store_dir = StoreDirectoryPath::from_opt_path(args.store_dir)?;
    let model: ModelKind = args.model_name.parse()?;

    let model_manifests = model::load_manifests()?;
    let manifest = model_manifests
        .get(model.to_name())
        .ok_or(anyhow::anyhow!("Could not find model: {}", model))?;

    let backend = model::FSBackend::new();
    let mut store = model::Store::new(store_dir, manifest, backend);

    store.ensure_dir().with_context(|| {
        format!("Failed to create \"{}\"", store.model_path().display())
    })?;

    let _guard = store.acquire_lock()?;

    // Here we will also have a callbacks setup.
    model::download_files(&mut store)?;

    Ok(())
}

fn list_models(_args: ListCommandArgs) -> anyhow::Result<()> {
    let model_manifests = model::load_manifests()?;

    let headers = vec![
        "model".to_string(),
        "size on disk".to_string(),
        "license".to_string(),
        "homepage".to_string(),
    ];

    let mut table = vec![headers];

    // TODO: List installed thingies for filtering.

    table.extend(model_manifests.into_values().map(|f| {
        let size_on_disk = f.size_on_disk();

        vec![
            f.name.to_string(),
            format_disk_size(size_on_disk, SizeBase::Base2),
            f.license_name,
            f.homepage_url.to_string(),
        ]
    }));

    print_format_table(&table, 2);
    Ok(())
}
