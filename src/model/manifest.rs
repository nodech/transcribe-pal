use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{
    model::{
        FileSize,
        hash::{Hash, Sha1, Sha256},
        line_parser::{LineParseError, LineParser},
    },
    transcribe::ModelKind,
};

pub type ModelVersion = String;

#[derive(Debug, thiserror::Error)]
pub enum ModelManifestParseError {
    #[error(transparent)]
    LineParseError(#[from] LineParseError),

    #[error("Unsupported version {0}")]
    UnsupportedVersion(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManifestPath(PathBuf);

impl ManifestPath {
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

#[derive(Debug)]
pub struct ModelManifestFile {
    pub path: ManifestPath,
    pub hash: Hash<Sha256>,
    pub size: FileSize,
}

fn parse_download_file(
    parser: &mut LineParser<'_>,
) -> Result<Option<ModelManifestFile>, ModelManifestParseError> {
    parser
        .next()
        .map(
            |line| -> Result<ModelManifestFile, ModelManifestParseError> {
                let file_name = line.value;
                let file_hash = parser.hash::<Sha256>("file_hash")?;
                let size = parser.usize("file_size")?;

                Ok(ModelManifestFile {
                    path: ManifestPath(PathBuf::from(file_name)),
                    hash: file_hash,
                    size: size as FileSize,
                })
            },
        )
        .transpose()
}

// TODO: Rename this to ModelManifestV1 and wrap it with enum?
#[derive(Debug)]
pub struct ModelManifest {
    /// This refers to the encoding format for this struct/manifest file.
    pub version: usize,
    pub model_version: ModelVersion,
    pub name: ModelKind,
    pub license_name: String,
    pub license_url: url::Url,
    pub homepage_url: url::Url,

    pub download_url: url::Url,
    pub download_hash: Hash<Sha1>,
    pub download_files: BTreeMap<ManifestPath, ModelManifestFile>,
}

impl ModelManifest {
    pub fn size_on_disk(&self) -> u64 {
        self.download_files.iter().map(|f| f.1.size).sum()
    }
}

impl FromStr for ModelManifest {
    type Err = ModelManifestParseError;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let mut parser = LineParser::new(data.lines());

        let version = parser.usize("version")?;

        if version != 1 {
            return Err(ModelManifestParseError::UnsupportedVersion(version));
        }

        let model_version = parser.string("model_version")?;
        let name = parser.model_kind("name")?;
        let license_name = parser.string("license_name")?;
        let license_url = parser.url("license_url")?;
        let homepage_url = parser.url("homepage_url")?;

        let download_url = parser.url("download_url")?;
        let download_hash = parser.hash::<Sha1>("download_hash")?;
        let mut download_files = BTreeMap::new();

        while let Some(file) = parse_download_file(&mut parser)? {
            download_files.insert(file.path.clone(), file);
        }

        Ok(Self {
            version,
            model_version,
            name,
            license_name,
            license_url,
            homepage_url,

            download_url,
            download_hash,
            download_files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(s: &str) -> ManifestPath {
        ManifestPath(PathBuf::from(s))
    }

    #[test]
    fn decode_manifest() {
        const MANIFEST: &str = "1
1.0.0
parakeet
license-name
https://license-url/
https://homepage-url/
https://download-url/
0000000000000000000000000000000000000001
file1
0000000000000000000000000000000000000000000000000000000000000002
512
file2.onnx
0000000000000000000000000000000000000000000000000000000000000003
256
";
        let manifest: ModelManifest = MANIFEST.parse().unwrap();

        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.model_version, "1.0.0");
        assert_eq!(manifest.name, ModelKind::Parakeet);
        assert_eq!(manifest.license_name, "license-name");
        assert_eq!(manifest.license_url.as_str(), "https://license-url/");
        assert_eq!(manifest.homepage_url.as_str(), "https://homepage-url/");
        // assert_eq!(manifest.size_on_disk, 768);
        assert_eq!(manifest.download_url.as_str(), "https://download-url/");
        assert_eq!(
            manifest.download_hash.as_str(),
            "0000000000000000000000000000000000000001"
        );
        assert_eq!(manifest.download_files.len(), 2);

        assert_eq!(manifest.download_files[&path("file1")].path, path("file1"));

        assert_eq!(
            manifest.download_files[&path("file1")].hash.as_str(),
            "0000000000000000000000000000000000000000000000000000000000000002"
        );
        assert_eq!(manifest.download_files[&path("file1")].size, 512);

        assert_eq!(
            manifest.download_files[&path("file2.onnx")].path,
            path("file2.onnx")
        );
        assert_eq!(
            manifest.download_files[&path("file2.onnx")].hash.as_str(),
            "0000000000000000000000000000000000000000000000000000000000000003"
        );
        assert_eq!(manifest.download_files[&path("file2.onnx")].size, 256);
    }
}
