use std::str::FromStr;

use crate::model::line_parser::{LineParseError, LineParser, ParsedLine};

#[derive(Debug, thiserror::Error)]
pub enum ModelManifestParseError {
    #[error(transparent)]
    LineParseError(#[from] LineParseError),

    #[error("Expected hash + filename found \"{found}\" at line {line_no}")]
    ExpectedHashAndFilename { found: String, line_no: usize },

    #[error("Expected hash found \"{found}\" at line {line_no}")]
    ExpectedHash { found: String, line_no: usize },

    #[error("Expected filename at line {0}")]
    ExpectedFilename(usize),

    #[error("Unsupported version {0}")]
    UnsupportedVersion(usize),
}

#[derive(Debug)]
pub struct ModelManifestFile {
    pub filename: String,
    pub hash: String,
}

fn parse_download_file(
    parser: &mut LineParser<'_>,
) -> Result<Option<ModelManifestFile>, ModelManifestParseError> {
    parser
        .next()
        .map(|ParsedLine { value, line_no }| {
            let trimmed = value.trim();
            let first_ws = trimmed.find(char::is_whitespace).ok_or(
                ModelManifestParseError::ExpectedHashAndFilename {
                    found: value.clone(),
                    line_no,
                },
            )?;

            let hash = trimmed[0..first_ws].trim();

            if hash.len() != 64 {
                return Err(ModelManifestParseError::ExpectedHash {
                    found: hash.into(),
                    line_no,
                });
            }

            let filename = trimmed[first_ws..].trim();

            // This will not happen.
            if filename.is_empty() {
                return Err(ModelManifestParseError::ExpectedFilename(line_no));
            }

            Ok(ModelManifestFile {
                hash: hash.to_string(),
                filename: filename.to_string(),
            })
        })
        .transpose()
}

// TODO: Rename this to ModelManifestV1 and wrap it with enum?
#[derive(Debug)]
pub struct ModelManifest {
    /// This refers to the encoding format for this struct/manifest file.
    pub version: usize,
    pub name: String,
    pub license_name: String,
    pub license_url: String,
    pub homepage_url: String,

    pub size_on_disk: usize,
    pub download_url: String,
    pub download_hash: String,
    pub download_files: Vec<ModelManifestFile>,
}

impl FromStr for ModelManifest {
    type Err = ModelManifestParseError;

    fn from_str(data: &str) -> Result<Self, Self::Err> {
        let mut parser = LineParser::new(data.lines());

        let version = parser.usize("version")?;

        if version != 1 {
            return Err(ModelManifestParseError::UnsupportedVersion(version));
        }

        let name = parser.string("name")?;
        let license_name = parser.string("license_name")?;
        let license_url = parser.string("license_url")?;
        let homepage_url = parser.string("homepage_url")?;

        let size_on_disk = parser.usize("size_on_disk")?;
        let download_url = parser.string("download_url")?;
        let download_hash = parser.string("download_hash")?;
        let mut download_files = vec![];

        while let Some(file) = parse_download_file(&mut parser)? {
            download_files.push(file);
        }

        Ok(Self {
            version,
            name,
            license_name,
            license_url,
            homepage_url,

            size_on_disk,
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
model-name
license-name
license-url
homepage-url
1024
download-url
0000000000000000000000000000000000000001
0000000000000000000000000000000000000000000000000000000000000002  file1
0000000000000000000000000000000000000000000000000000000000000003  file2.onnx
";
        let manifest: ModelManifest = MANIFEST.parse().unwrap();

        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.name, "model-name");
        assert_eq!(manifest.license_name, "license-name");
        assert_eq!(manifest.license_url, "license-url");
        assert_eq!(manifest.homepage_url, "homepage-url");
        assert_eq!(manifest.size_on_disk, 1024);
        assert_eq!(manifest.download_url, "download-url");
        assert_eq!(
            manifest.download_hash,
            "0000000000000000000000000000000000000001"
        );
        assert_eq!(manifest.download_files.len(), 2);

        assert_eq!(manifest.download_files[0].filename, "file1");
        assert_eq!(
            manifest.download_files[0].hash,
            "0000000000000000000000000000000000000000000000000000000000000002"
        );

        assert_eq!(manifest.download_files[1].filename, "file2.onnx");
        assert_eq!(
            manifest.download_files[1].hash,
            "0000000000000000000000000000000000000000000000000000000000000003"
        );
    }
}
