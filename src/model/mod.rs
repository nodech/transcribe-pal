use std::collections::HashMap;

use thiserror::Error;

mod download;
mod file_path;
mod hash;
mod line_parser;
mod manifest;
mod store;

pub use download::{Download, DownloadProgress, DownloadRequest};
pub use store::{FSBackend, ModelStore, Store, StoreDirectoryPath};

use file_path::FilePath;
pub use manifest::{ModelManifest, ModelManifestParseError};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_manifests() {
        load_manifests().unwrap();
    }
}
