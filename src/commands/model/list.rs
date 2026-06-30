use std::path::PathBuf;

use clap::Args;

use crate::{
    format::{SizeBase, format_disk_size, print_format_table},
    model::{
        self, Backend, FileSize, ModelManifest, ModelStore, StoreDirectoryPath,
    },
    transcribe::ModelKind,
};

#[derive(Debug, Args)]
pub(crate) struct ListCommandArgs {
    /// Model store directory
    #[arg(long)]
    store_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
enum InstallStatus {
    None,
    Partial,
    Installed,
}

struct TableEntry {
    model: ModelKind,
    model_size: f64,
    install_status: InstallStatus,
    disk_size: f64,
    license_name: Option<String>,
    homepage: Option<url::Url>,
}

const TABLE_COLUMNS: usize = 6;

struct Column {
    name: &'static str,
    value: fn(&mut TableEntry) -> String,
}

const COLUMNS: [Column; TABLE_COLUMNS] = [
    Column {
        name: "model",
        value: |t| t.model.to_string(),
    },
    Column {
        name: "model size",
        value: |t| size_to_string(t.model_size),
    },
    Column {
        name: "status",
        value: |t| match t.install_status {
            InstallStatus::None => "Not installed".to_string(),
            InstallStatus::Partial => "partial".to_string(),
            InstallStatus::Installed => "installed".to_string(),
        },
    },
    Column {
        name: "on disk size",
        value: |t| size_to_string(t.disk_size),
    },
    Column {
        name: "license",
        value: |t| t.license_name.take().expect("License was already taken."),
    },
    Column {
        name: "homepage",
        value: |t| {
            t.homepage
                .take()
                .map(String::from)
                .expect("Homepage was already taken.")
        },
    },
];

fn size_to_string(size: f64) -> String {
    let (size, unit) = format_disk_size(size, SizeBase::Base2);
    format!("{size} {unit}")
}

fn headers() -> [String; TABLE_COLUMNS] {
    COLUMNS.map(|c| c.name.to_string())
}

fn row(mut entry: TableEntry) -> [String; TABLE_COLUMNS] {
    COLUMNS.map(|c| (c.value)(&mut entry))
}

pub(super) fn list_models(args: ListCommandArgs) -> anyhow::Result<()> {
    let store_dir = StoreDirectoryPath::from_opt_path(args.store_dir)?;
    let model_manifests = model::load_manifests()?;

    let backend = model::FSBackend::new();
    let mut store = model::Store::new(store_dir, backend);

    let mut table: Vec<Vec<String>> = vec![headers().to_vec()];

    table.extend(
        model_manifests
            .into_values()
            .map(|m| {
                let (install_status, disk_size) = store_info(&mut store, &m)?;

                Ok(TableEntry {
                    model: m.name,
                    model_size: m.size_on_disk() as f64,
                    license_name: Some(m.license_name),
                    homepage: Some(m.homepage_url),
                    install_status,
                    disk_size,
                })
            })
            .map(|r| r.map(|m| row(m).to_vec()))
            .collect::<Result<Vec<_>, anyhow::Error>>()?,
    );

    print_format_table(&table, 2);
    Ok(())
}

fn store_info<T: Backend>(
    store: &mut model::Store<T>,
    manifest: &ModelManifest,
) -> Result<(InstallStatus, f64), anyhow::Error> {
    let mut model_store = ModelStore::from_store(store, manifest);

    if !model_store.exists() {
        return Ok((InstallStatus::None, 0.0));
    }

    let expected_size: FileSize = manifest.size_on_disk();
    let list = model_store.list_dir()?;
    let sum: FileSize = list.iter().map(|v| v.1.size).sum();

    if expected_size == sum {
        Ok((InstallStatus::Installed, sum as f64))
    } else if sum == 0 {
        Ok((InstallStatus::None, 0.0))
    } else {
        Ok((InstallStatus::Partial, sum as f64))
    }
}
