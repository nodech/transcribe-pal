use std::error::Error;
use std::ops::Mul;
use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;
use tracing::{debug, instrument, trace};
use transcribe_rs::onnx::Quantization;
use transcribe_rs::onnx::parakeet::ParakeetModel;
use transcribe_rs::transcriber::{Transcriber, VadChunked, VadChunkedConfig};
use transcribe_rs::vad;
use transcribe_rs::{SpeechModel, TranscribeError, TranscribeOptions};

use super::audio::{self, AudioConsumer, SampleRate};

const FRAME_GRANULAR_DUR_MS: u64 = 30;

// Min = 5 * 30ms = 150ms (5 * 480)
const MIN_SPEECH_END_DELAY: u64 = 150;

// Max = 120 * 30ms = 3200ms, 3.2s
const MAX_SPEECH_END_DELAY: u64 = 1800;

pub trait TranscriptWriter {
    type Error: Error + Sync + Send + 'static;

    fn push_text(&mut self, text: &str) -> Result<(), Self::Error>;
    fn flush(&mut self) -> Result<(), Self::Error>;
    fn finish(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum AudioTranscriberError<WE: Error + Sync + Send + 'static> {
    #[error("Failed to feed samples to the model: {0}")]
    FeedFailed(#[from] TranscribeError),

    #[error("Writer failed: {0}")]
    WriterFailed(WE),
}

pub struct AudioTranscriber<W: TranscriptWriter> {
    model: Box<dyn SpeechModel>,
    writer: W,
    chunked: VadChunked,
    skip_segments: usize,
}

impl<W> AudioTranscriber<W>
where
    W: TranscriptWriter,
{
    fn new(
        model: Box<dyn SpeechModel>,
        writer: W,
        chunked: VadChunked,
    ) -> Self {
        Self {
            model,
            writer,
            chunked,

            skip_segments: 0,
        }
    }
}

impl<W: TranscriptWriter> AudioConsumer for AudioTranscriber<W> {
    type Error = AudioTranscriberError<W::Error>;

    #[instrument(level = "trace", name = "transcriber.push_chunk", skip_all)]
    fn push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error> {
        let model = self.model.as_mut();
        let chunked = &mut self.chunked;
        let writer = &mut self.writer;

        trace!(samples_len = samples.len(), "Feeding model samples.");

        let results = chunked.feed(model, samples)?;
        trace!(text_chunks = results.len(), "Printing resulting text.");

        for result in results {
            writer
                .push_text(&result.text)
                .map_err(AudioTranscriberError::WriterFailed)?;
            writer
                .push_text(" ")
                .map_err(AudioTranscriberError::WriterFailed)?;

            if let Some(segments) = result.segments {
                self.skip_segments += segments.len();
            }
        }

        trace!("Flushing text.");
        writer
            .flush()
            .map_err(AudioTranscriberError::WriterFailed)?;

        Ok(())
    }

    #[instrument(
        level = "trace",
        name = "transcriber.finish",
        skip_all,
        fields(skipped_segments = self.skip_segments)
    )]
    fn finish(&mut self) -> Result<(), Self::Error> {
        let model = self.model.as_mut();
        let chunked = &mut self.chunked;
        let writer = &mut self.writer;
        let skipped = self.skip_segments;

        let finished = chunked.finish(model)?;

        trace!("finishing transcription");

        if let Some(segments) = finished.segments {
            for segment in segments.iter().skip(skipped) {
                writer
                    .push_text(&segment.text)
                    .map_err(AudioTranscriberError::WriterFailed)?;
            }
        }

        writer
            .push_text("\n")
            .map_err(AudioTranscriberError::WriterFailed)?;
        writer
            .finish()
            .map_err(AudioTranscriberError::WriterFailed)?;

        Ok(())
    }
}

#[derive(Debug)]
pub enum ModelKind {
    Parakeet,
}

impl ModelKind {
    pub fn audio_config(&self) -> audio::DeviceConfig {
        match self {
            ModelKind::Parakeet => audio::DeviceConfig {
                sample_rate: 16_000,
                channels: 1,
                format: audio::SampleFormat::F32,
                frames_per_buffer: 4096,
            },
        }
    }
}

#[derive(Debug)]
pub struct ModelConfig {
    kind: ModelKind,
    path: PathBuf,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            kind: ModelKind::Parakeet,
            path: PathBuf::from("models/parakeet"),
        }
    }
}

impl ModelConfig {
    pub fn audio_conig(&self) -> audio::DeviceConfig {
        self.kind.audio_config()
    }

    pub fn with_path_opt(mut self, path: Option<impl Into<PathBuf>>) -> Self {
        if let Some(path) = path {
            self.path = path.into();
        }

        self
    }

    pub fn with_kind(mut self, kind: impl Into<ModelKind>) -> Self {
        self.kind = kind.into();
        self
    }
}

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("Invalid threshold {0}, it should be in range from 0.0 to 1.0")]
    InvalidThreshold(f32),

    #[error("Invalid speech end delay {0:?}, it must be within 150..=3200 ms")]
    InvalidSpeechEndDelay(Duration),

    #[error("Could not load model: {0}")]
    FailedToLoad(#[from] TranscribeError),
}

#[derive(Debug)]
pub struct AudioTranscriberBuilder {
    language: Option<String>,
    model: ModelConfig,

    mic_threshold: f32,
    speech_end_delay: Duration,
}

impl Default for AudioTranscriberBuilder {
    fn default() -> Self {
        Self {
            language: None,

            mic_threshold: 0.01,
            speech_end_delay: Duration::from_millis(1000),

            model: Default::default(),
        }
    }
}

impl AudioTranscriberBuilder {
    #[allow(dead_code)]
    pub fn with_language(mut self, lang: String) -> Self {
        self.language = Some(lang);
        self
    }

    pub fn try_with_mic_threshold(
        mut self,
        threshold: f32,
    ) -> Result<Self, BuilderError> {
        if !(0.0..=1.0).contains(&threshold) {
            return Err(BuilderError::InvalidThreshold(threshold));
        }

        self.mic_threshold = threshold;
        Ok(self)
    }

    pub fn try_with_mic_threshold_opt(
        self,
        threshold: Option<f32>,
    ) -> Result<Self, BuilderError> {
        if let Some(threshold) = threshold {
            return self.try_with_mic_threshold(threshold);
        }

        Ok(self)
    }

    pub fn try_with_speech_end_delay(
        mut self,
        delay: Duration,
    ) -> Result<Self, BuilderError> {
        if !(Duration::from_millis(MIN_SPEECH_END_DELAY)
            ..=Duration::from_millis(MAX_SPEECH_END_DELAY))
            .contains(&delay)
        {
            return Err(BuilderError::InvalidSpeechEndDelay(delay));
        }

        self.speech_end_delay = delay;
        Ok(self)
    }

    pub fn try_with_speech_end_delay_opt(
        self,
        delay: Option<Duration>,
    ) -> Result<Self, BuilderError> {
        if let Some(delay) = delay {
            return self.try_with_speech_end_delay(delay);
        }

        Ok(self)
    }

    pub fn with_model(mut self, model: ModelConfig) -> Self {
        self.model = model;
        self
    }

    fn setup_model(&self) -> Result<Box<dyn SpeechModel>, BuilderError> {
        match self.model.kind {
            ModelKind::Parakeet => {
                let quantization = Quantization::Int8;
                let model =
                    ParakeetModel::load(&self.model.path, &quantization)?;

                Ok(Box::new(model))
            }
        }
    }

    #[instrument(
        level = "debug",
        name = "setup_chunked",
        skip_all,
        fields(speech_end_delay = ?self.speech_end_delay),
    )]
    fn setup_chunked(&self) -> VadChunked {
        let options = TranscribeOptions {
            language: self.language.clone(),
            ..Default::default()
        };

        let model_audio_cfg = self.model.kind.audio_config();
        let frame_granular_duration =
            Duration::from_millis(FRAME_GRANULAR_DUR_MS);

        // e.g. 480 for 30 ms at 16k
        let frame_size = frame_size_for_dur(
            frame_granular_duration,
            model_audio_cfg.sample_rate,
        );

        debug!(
            audio_config = ?model_audio_cfg,
            frame_granular_duration = ?frame_granular_duration,
            frame_size = frame_size,
            mic_threshold = self.mic_threshold,
            "setting up vad chunker"
        );

        let envad =
            Box::new(vad::EnergyVad::new(frame_size, self.mic_threshold));

        // Min = 5 * 30ms = 150ms (5 * 480)
        // Max = 120 * 30ms = 3200ms, 3.2s
        let hangover_samples = frame_size_for_dur(
            self.speech_end_delay,
            model_audio_cfg.sample_rate,
        );

        let hangover_frames =
            hangover_samples.div_ceil(frame_size).clamp(5, 60);

        debug!(
            hangover_frames = hangover_frames,
            frame_size = frame_size,
            "setting up smooth vad"
        );

        // prefill = 20 * 30ms = 600ms
        let smooth_vad =
            Box::new(vad::SmoothedVad::new(envad, 20, hangover_frames, 2));

        let vad_chunked_config = VadChunkedConfig {
            min_chunk_secs: 1.0,
            max_chunk_secs: 45.0,
            padding_secs: 0.35,
            smart_split_search_secs: Some(2.0),
            merge_separator: " ".into(),
        };

        debug!(
            min_chunk_secs = vad_chunked_config.min_chunk_secs,
            max_chunk_secs = vad_chunked_config.max_chunk_secs,
            padding_secs = vad_chunked_config.padding_secs,
            smart_split_search_secs =
                vad_chunked_config.smart_split_search_secs,
            merge_separator = vad_chunked_config.merge_separator,
            "setting up vad chunked"
        );

        VadChunked::new(smooth_vad, vad_chunked_config, options)
    }

    #[instrument(level = "debug", name = "transcriber.build", skip_all)]
    pub fn build<W: TranscriptWriter>(
        self,
        writer: W,
    ) -> Result<AudioTranscriber<W>, BuilderError> {
        debug!(opts = ?&self, "building transcriber");
        let chunked = self.setup_chunked();
        debug!("setup chunked");
        let model = self.setup_model()?;
        debug!("setup model");

        Ok(AudioTranscriber::new(model, writer, chunked))
    }
}

fn frame_size_for_dur(dur: Duration, sample_rate_hz: SampleRate) -> usize {
    dur.as_millis()
        .mul(sample_rate_hz as u128)
        .div_ceil(1000)
        .max(1) as usize
}
