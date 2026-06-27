use std::collections::HashMap;

use thiserror::Error;

mod download;
mod file_path;
mod hash;
mod line_parser;
mod manifest;
mod store;

pub use store::{FSBackend, Store, StoreDirectoryPath};

use file_path::FilePath;
use manifest::{ModelManifest, ModelManifestParseError};
use tracing::{debug, instrument};

use crate::model::{
    download::{Download, DownloadError, DownloadRequest},
    store::Backend,
};

pub type FileSize = u64;
pub type ManifestMap = HashMap<&'static str, ModelManifest>;

#[derive(Debug, Error)]
#[error("Failed to parse \"{0}\" manifest: {1}")]
pub struct LoadManifestError(&'static str, ModelManifestParseError);

pub fn load_manifests() -> Result<ManifestMap, LoadManifestError> {
    let raw_manifests =
        [("parakeet", include_str!("./data/parakeet.manifest"))];

    let mut hash_map = HashMap::<&'static str, ModelManifest>::new();

    for (name, data) in raw_manifests.into_iter() {
        let manifest: ModelManifest =
            data.parse().map_err(|e| LoadManifestError(name, e))?;
        hash_map.insert(name, manifest);
    }

    Ok(hash_map)
}

#[derive(Debug, Error)]
pub enum DownloadFailed<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    #[error(transparent)]
    LoadManifestError(#[from] LoadManifestError),

    #[error(transparent)]
    DownloadStepFailed(#[from] DownloadError),

    #[error("Store Backend Error: {0}")]
    StoreBackend(E),
}

#[instrument(level = "debug", name = "download_files", skip_all)]
pub fn download_files<'m, T: Backend>(
    store: &mut store::Store<'m, T>,
) -> Result<(), DownloadFailed<T::Error>> {
    let download_request =
        DownloadRequest::new(store).map_err(DownloadFailed::StoreBackend)?;

    debug!("download request created");

    let mut downloader = Download::new(store, download_request);

    debug!("starting download");
    while let Some(mut file) = downloader.next() {
        debug!("processing: {:?}", file.file_path());

        while let Some(s) = file.process()? {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_manifests() {
        load_manifests().unwrap();
    }
}
