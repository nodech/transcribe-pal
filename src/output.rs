use super::transcribe::TranscriptWriter;
use std::io::{Stderr, Stdout, Write};

pub struct IoWriter<W: Write> {
    writer: W,
}

impl<W: Write> IoWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> TranscriptWriter for IoWriter<W> {
    fn push_text(&mut self, text: &str) -> anyhow::Result<()> {
        Ok(self.writer.write_all(text.as_bytes())?)
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        Ok(self.writer.flush()?)
    }

    fn finish(&mut self) -> anyhow::Result<()> {
        self.writer.flush()?;
        Ok(())
    }
}

impl IoWriter<Stdout> {
    pub fn stdout() -> Self {
        Self::new(std::io::stdout())
    }
}

impl IoWriter<Stderr> {
    pub fn stderr() -> Self {
        Self::new(std::io::stderr())
    }
}

pub struct MultiWriter {
    writers: Vec<Box<dyn TranscriptWriter>>,
}

impl TranscriptWriter for MultiWriter {
    fn push_text(&mut self, text: &str) -> anyhow::Result<()> {
        for writer in self.writers.iter_mut() {
            writer.push_text(text)?;
        }

        Ok(())
    }

    fn flush(&mut self) -> anyhow::Result<()> {
        for writer in self.writers.iter_mut() {
            writer.flush()?;
        }

        Ok(())
    }

    fn finish(&mut self) -> anyhow::Result<()> {
        for writer in self.writers.iter_mut() {
            writer.finish()?;
        }

        Ok(())
    }
}

impl MultiWriter {
    pub fn new(writers: Vec<Box<dyn TranscriptWriter>>) -> Self {
        Self { writers }
    }
}
