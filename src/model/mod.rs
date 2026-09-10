use std::{collections::HashMap, sync::OnceLock};

use thiserror::Error;

mod download;
mod file_path;
mod hash;
mod line_parser;
mod manifest;
mod store;

pub use download::{Download, DownloadProgress, DownloadRequest};
pub use store::{
    Backend, FSBackend, ModelStore, ModelStoreStatus, Store,
    StoreDirectoryPath,
};

use file_path::FilePath;
pub use manifest::{ModelManifest, ModelManifestParseError};

#[derive(Debug, Error)]
#[error("Failed to parse \"{0}\" manifest: {1}")]
pub struct LoadManifestError(&'static str, ModelManifestParseError);

pub type FileSize = u64;
pub type ManifestMap = HashMap<&'static str, ModelManifest>;

// Name -> Content
type RawManifest = (&'static str, &'static str);

static RAW_MANIFESTS: [RawManifest; 1] =
    [("parakeet", include_str!("./data/parakeet.manifest"))];

static MANIFEST_MAP: OnceLock<Result<ManifestMap, LoadManifestError>> =
    OnceLock::new();

fn parse_manifests() -> Result<ManifestMap, LoadManifestError> {
    let mut hash_map = ManifestMap::new();

    for (name, data) in RAW_MANIFESTS.iter() {
        let manifest = data.parse().map_err(|e| LoadManifestError(name, e))?;
        hash_map.insert(name, manifest);
    }

    Ok(hash_map)
}

pub fn load_manifests()
-> Result<&'static ManifestMap, &'static LoadManifestError> {
    MANIFEST_MAP.get_or_init(parse_manifests).as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_manifests() {
        load_manifests().unwrap();
    }
}
