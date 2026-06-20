use std::num::ParseIntError;

use crate::model::hash::{Hash, HashSize, IncorrectHash};

#[derive(Debug, thiserror::Error)]
pub enum LineParseError {
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

    #[error("Expected hash for {name}, got: {found} at line {line_no}")]
    ExpectedHash {
        name: &'static str,
        found: String,
        line_no: usize,
        #[source]
        source: IncorrectHash,
    },
}

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
    ) -> Result<ParsedLine<String>, LineParseError> {
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

    pub(super) fn usize_line(
        &mut self,
        name: &'static str,
    ) -> Result<ParsedLine<usize>, LineParseError> {
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

    pub(super) fn usize(
        &mut self,
        name: &'static str,
    ) -> Result<usize, LineParseError> {
        self.usize_line(name).map(|l| l.into_value())
    }

    pub(super) fn hash_line<T: HashSize>(
        &mut self,
        name: &'static str,
    ) -> Result<ParsedLine<Hash<T>>, LineParseError> {
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

    pub(super) fn hash<T: HashSize>(
        &mut self,
        name: &'static str,
    ) -> Result<Hash<T>, LineParseError> {
        self.hash_line(name).map(|l| l.into_value())
    }
}
