use std::{num::ParseIntError, str::FromStr};

#[derive(Debug, thiserror::Error)]
pub enum ModelManifestParseError {
    #[error("Expected line for \"{name}\" at line {line_no}")]
    MissingLine { name: &'static str, line_no: usize },

    #[error("Expected an integer for {name}, got: {found} at line {line_no}")]
    ExpectedInteger {
        name: &'static str,
        found: String,
        line_no: usize,
        #[source]
        source: ParseIntError,
    },

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

// TODO: Rename this to ModelManifestV1 and wrap it with enum?
#[derive(Debug)]
pub struct ModelManifest {
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

        while let Some(file) = parser.download_file()? {
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

struct LineParser<'a> {
    processed: usize,
    lines: std::str::Lines<'a>,
}

impl<'a> LineParser<'a> {
    fn new(lines_iter: std::str::Lines<'a>) -> Self {
        LineParser {
            processed: 0,
            lines: lines_iter,
        }
    }

    fn next(&mut self) -> Option<String> {
        self.lines.next().map(|l| {
            self.processed += 1;
            l.to_string()
        })
    }

    fn string(
        &mut self,
        name: &'static str,
    ) -> Result<String, ModelManifestParseError> {
        self.next().ok_or(ModelManifestParseError::MissingLine {
            name,
            line_no: self.processed,
        })
    }

    fn usize(
        &mut self,
        name: &'static str,
    ) -> Result<usize, ModelManifestParseError> {
        let line = self.string(name)?;

        line.parse::<usize>().map_err(|e| {
            ModelManifestParseError::ExpectedInteger {
                name,
                found: line,
                line_no: self.processed,
                source: e,
            }
        })
    }

    fn download_file(
        &mut self,
    ) -> Result<Option<ModelManifestFile>, ModelManifestParseError> {
        self.next()
            .map(|l| {
                let trimmed = l.trim();
                let first_ws = trimmed.find(char::is_whitespace).ok_or(
                    ModelManifestParseError::ExpectedHashAndFilename {
                        found: l.clone(),
                        line_no: self.processed,
                    },
                )?;

                let hash = trimmed[0..first_ws].trim();

                if hash.len() != 64 {
                    return Err(ModelManifestParseError::ExpectedHash {
                        found: hash.into(),
                        line_no: self.processed,
                    });
                }

                let filename = trimmed[first_ws..].trim();

                // This will not happen.
                if filename.is_empty() {
                    return Err(ModelManifestParseError::ExpectedFilename(
                        self.processed,
                    ));
                }

                Ok(ModelManifestFile {
                    hash: hash.to_string(),
                    filename: filename.to_string(),
                })
            })
            .transpose()
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
