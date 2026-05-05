use std::path::PathBuf;

use anyhow::Context;
use transcribe_rs::TranscribeOptions;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::onnx::parakeet::ParakeetModel;
use transcribe_rs::transcriber::{VadChunked, VadChunkedConfig};
use transcribe_rs::vad::{EnergyVad, SmoothedVad};

pub trait TranscriptWriter {
    fn push_text(&mut self, text: &str) -> anyhow::Result<()>;
    fn flush(&mut self) -> anyhow::Result<()>;
    fn finish(&mut self) -> anyhow::Result<()>;
}

// Parakeet only for now, we'll see in the future.
pub fn chunked_transcriber() -> anyhow::Result<VadChunked> {
    let options = TranscribeOptions::default();
    let envad = Box::new(EnergyVad::new(480, 0.01));
    let vad = Box::new(SmoothedVad::new(envad, 15, 15, 2));

    Ok(VadChunked::new(vad, VadChunkedConfig::default(), options))
}

// Parakeet for now, we'll see in the future.
pub fn setup_model() -> anyhow::Result<ParakeetModel> {
    ParakeetModel::load(&PathBuf::from("models/parakeet"), &Quantization::Int8)
        .with_context(|| "Failed to load parakeet model at models/parakeet")
}
