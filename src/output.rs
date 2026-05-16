use super::transcribe::TranscriptWriter;
use anyhow::Result;
use std::{
    error::Error,
    io::{Stderr, Stdout, Write},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IoWriterError {
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),
}

pub struct IoWriter<W: Write> {
    writer: W,
}

impl<W: Write> IoWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> TranscriptWriter for IoWriter<W> {
    type Error = IoWriterError;

    fn push_text(&mut self, text: &str) -> Result<(), Self::Error> {
        Ok(self.writer.write_all(text.as_bytes())?)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(self.writer.flush()?)
    }

    fn finish(&mut self) -> Result<(), Self::Error> {
        self.writer.flush()?;
        Ok(())
    }
}

impl IoWriter<Stdout> {
    pub fn stdout() -> Self {
        Self::new(std::io::stdout())
    }
}

#[allow(dead_code)]
impl IoWriter<Stderr> {
    pub fn stderr() -> Self {
        Self::new(std::io::stderr())
    }
}

// Box<dyn Error> does not implement Error unfortunately:
// https://github.com/rust-lang/rust/issues/60759
type GenericError = Box<dyn Error + Send + Sync + 'static>;

// So we wrap it.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct MultiWriteError(#[from] GenericError);

trait MultiWriteWrapper {
    fn push_text_erased(&mut self, text: &str) -> Result<(), GenericError>;
    fn flush_erased(&mut self) -> Result<(), GenericError>;
    fn finish_erased(&mut self) -> Result<(), GenericError>;
}

impl<T> MultiWriteWrapper for T
where
    T: TranscriptWriter,
{
    fn push_text_erased(&mut self, text: &str) -> Result<(), GenericError> {
        self.push_text(text).map_err(Into::into)
    }

    fn flush_erased(&mut self) -> Result<(), GenericError> {
        self.flush().map_err(Into::into)
    }

    fn finish_erased(&mut self) -> Result<(), GenericError> {
        self.finish().map_err(Into::into)
    }
}

pub struct MultiWriter {
    writers: Vec<Box<dyn MultiWriteWrapper + Send>>,
}

impl TranscriptWriter for MultiWriter {
    type Error = MultiWriteError;

    fn push_text(&mut self, text: &str) -> Result<(), Self::Error> {
        for writer in self.writers.iter_mut() {
            writer.push_text_erased(text)?;
        }

        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        for writer in self.writers.iter_mut() {
            writer.flush_erased()?;
        }

        Ok(())
    }

    fn finish(&mut self) -> Result<(), Self::Error> {
        for writer in self.writers.iter_mut() {
            writer.finish_erased()?;
        }

        Ok(())
    }
}

impl MultiWriter {
    pub fn new() -> Self {
        Self { writers: vec![] }
    }

    pub fn push_writer<W>(&mut self, writer: W)
    where
        W: TranscriptWriter + Send + 'static,
    {
        self.writers.push(Box::new(writer));
    }

    pub fn is_empty(&self) -> bool {
        self.writers.is_empty()
    }
}
