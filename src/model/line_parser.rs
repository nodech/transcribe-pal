use std::num::ParseIntError;

use crate::{
    model::hash::{Hash, HashKind, IncorrectHash},
    transcribe::{ModelKind, ModelKindUnknown},
};

use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum LineParseError {
    #[error("Expected line for \"{name}\" at line {line_no}")]
    MissingLine { name: &'static str, line_no: usize },

    #[error(
        "Expected an integer for \"{name}\", got: {found} at line {line_no}"
    )]
    ExpectedInteger {
        name: &'static str,
        found: String,
        line_no: usize,
        #[source]
        source: ParseIntError,
    },

    #[error("Expected hash for \"{name}\", got: {found} at line {line_no}")]
    ExpectedHash {
        name: &'static str,
        found: String,
        line_no: usize,
        #[source]
        source: IncorrectHash,
    },

    #[error("Uknown model name \"{name}\" at line {line_no}")]
    UnknownModelName {
        name: &'static str,
        line_no: usize,
        #[source]
        source: ModelKindUnknown,
    },

    #[error("Expected url for \"{name}\", got: {found} at line {line_no}")]
    ExpectedUrl {
        name: &'static str,
        found: String,
        line_no: usize,
        #[source]
        source: url::ParseError,
    },
}

type LineResult<T> = Result<T, LineParseError>;

pub(super) struct LineParser<'a> {
    processed: usize,
    lines: std::str::Lines<'a>,
}

pub(super) struct ParsedLine<T> {
    pub(super) value: T,
    pub(super) line_no: usize,
}

impl<T> ParsedLine<T> {
    fn into_value(self) -> T {
        self.value
    }
}

impl<'a> LineParser<'a> {
    pub(super) fn new(lines_iter: std::str::Lines<'a>) -> Self {
        LineParser {
            processed: 0,
            lines: lines_iter,
        }
    }

    pub(super) fn next(&mut self) -> Option<ParsedLine<String>> {
        self.lines.next().map(|l| {
            self.processed += 1;

            ParsedLine {
                value: l.to_string(),
                line_no: self.processed,
            }
        })
    }

    pub(super) fn string_line(
        &mut self,
        name: &'static str,
    ) -> LineResult<ParsedLine<String>> {
        self.next().ok_or_else(|| LineParseError::MissingLine {
            name,
            line_no: self.processed + 1,
        })
    }

    pub(super) fn string(
        &mut self,
        name: &'static str,
    ) -> Result<String, LineParseError> {
        self.string_line(name).map(|l| l.into_value())
    }

    pub(super) fn model_kind_line(
        &mut self,
        name: &'static str,
    ) -> LineResult<ParsedLine<ModelKind>> {
        let line = self.string_line(name)?;

        line.value
            .parse()
            .map(|n| ParsedLine {
                value: n,
                line_no: line.line_no,
            })
            .map_err(|e| LineParseError::UnknownModelName {
                name,
                line_no: line.line_no,
                source: e,
            })
    }

    pub(super) fn model_kind(
        &mut self,
        name: &'static str,
    ) -> LineResult<ModelKind> {
        self.model_kind_line(name).map(|l| l.into_value())
    }

    pub(super) fn usize_line(
        &mut self,
        name: &'static str,
    ) -> LineResult<ParsedLine<usize>> {
        let parsed = self.string_line(name)?;

        parsed
            .value
            .parse::<usize>()
            .map(|n| ParsedLine {
                value: n,
                line_no: parsed.line_no,
            })
            .map_err(|e| LineParseError::ExpectedInteger {
                name,
                found: parsed.value,
                line_no: parsed.line_no,
                source: e,
            })
    }

    pub(super) fn usize(&mut self, name: &'static str) -> LineResult<usize> {
        self.usize_line(name).map(|l| l.into_value())
    }

    pub(super) fn hash_line<T: HashKind>(
        &mut self,
        name: &'static str,
    ) -> LineResult<ParsedLine<Hash<T>>> {
        let parsed = self.string_line(name)?;

        parsed
            .value
            .parse::<Hash<T>>()
            .map(|n| ParsedLine {
                value: n,
                line_no: parsed.line_no,
            })
            .map_err(|e| LineParseError::ExpectedHash {
                name,
                found: parsed.value,
                line_no: parsed.line_no,
                source: e,
            })
    }

    pub(super) fn hash<T: HashKind>(
        &mut self,
        name: &'static str,
    ) -> LineResult<Hash<T>> {
        self.hash_line(name).map(|l| l.into_value())
    }

    pub(super) fn url_line(
        &mut self,
        name: &'static str,
    ) -> LineResult<ParsedLine<url::Url>> {
        let parsed = self.string_line(name)?;

        Url::parse(&parsed.value)
            .map(|u| ParsedLine {
                value: u,
                line_no: parsed.line_no,
            })
            .map_err(|e| LineParseError::ExpectedUrl {
                name,
                found: parsed.value,
                line_no: parsed.line_no,
                source: e,
            })
    }

    pub(super) fn url(&mut self, name: &'static str) -> LineResult<url::Url> {
        self.url_line(name).map(|l| l.into_value())
    }
}
