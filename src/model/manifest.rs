use std::str::FromStr;

use crate::model::{
    hash::{Hash, Sha1, Sha256},
    line_parser::{LineParseError, LineParser},
};

pub type ModelVersion = String;

#[derive(Debug, thiserror::Error)]
pub enum ModelManifestParseError {
    #[error(transparent)]
    LineParseError(#[from] LineParseError),

    #[error("Unsupported version {0}")]
    UnsupportedVersion(usize),
}

#[derive(Debug)]
pub struct ModelManifestFile {
    pub name: String,
    pub hash: Hash<Sha256>,
    pub size: usize,
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
                    name: file_name,
                    hash: file_hash,
                    size,
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
    pub name: String,
    pub license_name: String,
    pub license_url: String,
    pub homepage_url: String,

    pub download_url: String,
    pub download_hash: Hash<Sha1>,
    pub download_files: Vec<ModelManifestFile>,
}

impl ModelManifest {
    pub fn size_on_disk(&self) -> usize {
        self.download_files.iter().map(|f| f.size).sum()
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
        let name = parser.string("name")?;
        let license_name = parser.string("license_name")?;
        let license_url = parser.string("license_url")?;
        let homepage_url = parser.string("homepage_url")?;

        let download_url = parser.string("download_url")?;
        let download_hash = parser.hash::<Sha1>("download_hash")?;
        let mut download_files = vec![];

        while let Some(file) = parse_download_file(&mut parser)? {
            download_files.push(file);
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

    #[test]
    fn decode_manifest() {
        const MANIFEST: &str = "1
1.0.0
model-name
license-name
license-url
homepage-url
download-url
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
        assert_eq!(manifest.name, "model-name");
        assert_eq!(manifest.license_name, "license-name");
        assert_eq!(manifest.license_url, "license-url");
        assert_eq!(manifest.homepage_url, "homepage-url");
        // assert_eq!(manifest.size_on_disk, 768);
        assert_eq!(manifest.download_url, "download-url");
        assert_eq!(
            manifest.download_hash.as_str(),
            "0000000000000000000000000000000000000001"
        );
        assert_eq!(manifest.download_files.len(), 2);

        assert_eq!(manifest.download_files[0].name, "file1");
        assert_eq!(
            manifest.download_files[0].hash.as_str(),
            "0000000000000000000000000000000000000000000000000000000000000002"
        );
        assert_eq!(manifest.download_files[0].size, 512);

        assert_eq!(manifest.download_files[1].name, "file2.onnx");
        assert_eq!(
            manifest.download_files[1].hash.as_str(),
            "0000000000000000000000000000000000000000000000000000000000000003"
        );
        assert_eq!(manifest.download_files[1].size, 256);
    }
}
