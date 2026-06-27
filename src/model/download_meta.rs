use std::{fmt, str::FromStr};
use thiserror::Error;

use crate::model::{
    file_path::FilePath,
    line_parser::{LineParseError, LineParser},
    manifest::{ModelManifest, ModelVersion},
};

#[derive(Debug, PartialEq)]
pub struct FileEntry {
    path: FilePath,
    etag: String,
}

impl fmt::Display for FileEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.path)?;
        writeln!(f, "{}", self.etag)?;

        Ok(())
    }
}

fn parse_file_entry(
    parser: &mut LineParser,
) -> Result<Option<FileEntry>, ModelDetailsParseError> {
    parser
        .next()
        .map(|pline| -> Result<FileEntry, ModelDetailsParseError> {
            let file_name = pline.value;
            let etag = parser.string("etag")?;

            Ok(FileEntry {
                path: FilePath::new(file_name),
                etag,
            })
        })
        .transpose()
}

#[derive(Debug, Error)]
pub enum ModelDetailsParseError {
    #[error(transparent)]
    LineParseError(#[from] LineParseError),

    #[error("Unsupported model details version: {0}")]
    UnsupportedVersion(usize),
}

#[derive(Debug, PartialEq)]
pub struct Metadata {
    pub version: usize,
    pub model_version: ModelVersion,
    pub files: Vec<FileEntry>,
}

impl Metadata {
    pub fn new(manifest: &ModelManifest) -> Self {
        Metadata {
            version: 1,
            model_version: manifest.model_version.clone(),
            files: vec![],
        }
    }
}

impl fmt::Display for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.version)?;
        writeln!(f, "{}", self.model_version)?;

        for file in &self.files {
            write!(f, "{}", file)?;
        }

        Ok(())
    }
}

impl FromStr for Metadata {
    type Err = ModelDetailsParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parser = LineParser::new(s.lines());

        let version = parser.usize("version")?;

        if version != 1 {
            return Err(ModelDetailsParseError::UnsupportedVersion(version));
        }

        let model_version = parser.string("model_version")?;

        let mut files = vec![];

        while let Some(file_entry) = parse_file_entry(&mut parser)? {
            files.push(file_entry);
        }

        Ok(Metadata {
            version,
            model_version,
            files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_model_data1() -> (&'static str, Metadata) {
        const DATA1: &str = "1
0.0.0
filename1
ETAG1
filename2
ETAG2
";

        let model = Metadata {
            version: 1,
            model_version: "0.0.0".to_string(),
            files: vec![
                FileEntry {
                    path: FilePath::new("filename1".to_string()),
                    etag: "ETAG1".to_string(),
                },
                FileEntry {
                    path: FilePath::new("filename2".to_string()),
                    etag: "ETAG2".to_string(),
                },
            ],
        };

        (DATA1, model)
    }

    #[test]
    fn model_encode_data() {
        let (data, model) = get_model_data1();
        assert_eq!(model.to_string(), data);
    }

    #[test]
    fn model_decode_data() {
        let (data, model) = get_model_data1();
        let parsed: Metadata = data.parse().unwrap();

        assert_eq!(parsed, model)
    }

    #[test]
    fn model_expected_version() {
        let data = "";
        let parsed: ModelDetailsParseError =
            data.parse::<Metadata>().unwrap_err();

        assert!(matches!(
            parsed,
            ModelDetailsParseError::LineParseError(
                LineParseError::MissingLine {
                    name: "version",
                    line_no: 1
                }
            )
        ));

        let data = "bad";
        let parsed: ModelDetailsParseError =
            data.parse::<Metadata>().unwrap_err();

        assert!(matches!(
            parsed,
            ModelDetailsParseError::LineParseError(
                LineParseError::ExpectedInteger {
                    name: "version",
                    ref found,
                    line_no: 1,
                    source: _
                }
            ) if found == "bad"
        ));
    }

    #[test]
    fn model_expected_file_data() {
        let data = "1
0.0.0
First line";

        let parsed: ModelDetailsParseError =
            data.parse::<Metadata>().unwrap_err();

        assert!(matches!(
            parsed,
            ModelDetailsParseError::LineParseError(
                LineParseError::MissingLine {
                    name: "etag",
                    line_no: 4
                }
            )
        ))
    }
}
