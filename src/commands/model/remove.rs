use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::anyhow;
use clap::Args;

use crate::{
    model::{self, Backend, StoreDirectoryPath},
    transcribe::ModelKind,
};

#[derive(Debug, Args)]
pub(crate) struct RemoveCommandArgs {
    /// Model name
    model_name: Option<String>,

    /// Model store directory
    #[arg(long)]
    store_dir: Option<PathBuf>,

    /// Remove all models
    #[arg(long)]
    all: bool,
}

pub(super) fn remove_model(args: RemoveCommandArgs) -> anyhow::Result<()> {
    let store_dir = StoreDirectoryPath::from_opt_path(args.store_dir)?;

    let fs = model::FSBackend::new();
    let mut store = model::Store::new(store_dir, fs);

    let _guard = store.acquire_lock()?;

    if args.all {
        return remove_all(store);
    }

    let model: ModelKind = args
        .model_name
        .ok_or_else(|| anyhow!("provide model name or `--all`"))?
        .parse()?;

    let mut model_path = store.path().to_path_buf();
    model_path.push(model.to_name());

    if prompt(&format!("Removing {}", model_path.display()))? {
        store.remove_dir(&model_path).map_err(|e| {
            anyhow!("failed to remove \"{}\", err: {e}", model_path.display())
        })?;

        eprint!("Removed {}", model_path.display());
    }

    Ok(())
}

pub(super) fn remove_all<T: Backend>(
    mut store: model::Store<T>,
) -> anyhow::Result<()> {
    let dirs = store.list_dirs()?;

    for dir in dirs {
        if prompt(&format!("Removing {}", dir.display()))? {
            store.remove_dir(&dir).map_err(|e| {
                anyhow!("failed to remove \"{}\", err: {e}", dir.display())
            })?;

            eprintln!("Removed {}", dir.display());
        } else {
            eprintln!("Skipping {}", dir.display());
        }
    }

    Ok(())
}

fn prompt(text: &str) -> io::Result<bool> {
    eprintln!("{text}");
    eprint!("Continue ? [y/n]: ");
    io::stderr().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;

    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}
