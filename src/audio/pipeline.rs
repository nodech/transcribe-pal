use cpal::{FrameCount, SampleRate};
use tracing::{debug, instrument, trace, trace_span};

use crate::audio::{
    AudioConsumer, ChannelCount, DeviceConfig, RawBufferSize, SampleFormat,
};

#[derive(Debug, thiserror::Error)]
#[error("Pipieline error: {error}")]
pub struct PipelineError {
    error: Box<dyn std::error::Error + Sync + Send + 'static>,
}

impl PipelineError {
    pub fn new(error: impl std::error::Error + Sync + Send + 'static) -> Self {
        Self {
            error: Box::new(error),
        }
    }
}

/// Transform audio chunk
pub trait AudioProcessor {
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn process(
        &mut self,
        input: &[f32],
        output: &mut Vec<f32>,
    ) -> Result<(), PipelineError>;

    fn output_spec(&self) -> AudioSpec;

    fn finish(&mut self) -> Result<(), PipelineError>;
}

/// Audio Specification for the stage/processor
#[derive(Debug, Clone, Copy)]
pub struct AudioSpec {
    format: SampleFormat,
    sample_rate: SampleRate,
    channels: ChannelCount,
    frames_per_buffer: FrameCount,
}

impl AudioSpec {
    fn samples_per_buffer(&self) -> RawBufferSize {
        self.frames_per_buffer * (self.channels as RawBufferSize)
    }
}

impl From<DeviceConfig> for AudioSpec {
    fn from(value: DeviceConfig) -> Self {
        Self {
            format: value.format,
            sample_rate: value.sample_rate,
            channels: value.channels,
            frames_per_buffer: value.frames_per_buffer,
        }
    }
}

pub struct PipelineBuilder {
    current_spec: AudioSpec,
    max_buffer_size: RawBufferSize,
    stages: Vec<Box<dyn AudioProcessor + Send>>,
}

impl PipelineBuilder {
    #[instrument(level = "debug", name = "audio_pipeline.builder", skip_all)]
    pub fn new(device_config: impl Into<AudioSpec>) -> Self {
        let spec = device_config.into();

        debug!(
            channels = spec.channels,
            sample_rate = spec.sample_rate,
            buffer_size = spec.frames_per_buffer,
            "building audio pipeline"
        );

        Self {
            stages: vec![],
            current_spec: spec,
            max_buffer_size: spec.samples_per_buffer(),
        }
    }

    #[instrument(level = "debug", name = "audio_pipeline.builder", skip_all)]
    pub fn with_stage_fn<F, S>(
        mut self,
        init_stage: F,
    ) -> Result<Self, PipelineError>
    where
        S: AudioProcessor + Send + 'static,
        F: FnOnce(AudioSpec) -> Result<Option<S>, PipelineError>,
    {
        debug!(spec = ?self.current_spec, "adding new stage");
        let stage = init_stage(self.current_spec)?;

        if let Some(stage) = stage {
            // Recalculate new spec and max_buffer_size for each stage.
            self.current_spec = stage.output_spec();
            self.max_buffer_size = self
                .max_buffer_size
                .max(self.current_spec.samples_per_buffer());

            debug!(
                spec = ?self.current_spec,
                max_buf_size = self.max_buffer_size,
                name = stage.name(),
                "added new stage"
            );

            self.stages.push(Box::new(stage));
        }

        Ok(self)
    }

    #[instrument(level = "debug", name = "audio_pipeline.builder", skip_all)]
    pub fn build<T: AudioConsumer + Send>(self, sink: T) -> Pipeline<T> {
        debug!(
            stages = self.stages.len(),
            max_buffer_size = self.max_buffer_size,
            "built audio pipeline"
        );

        Pipeline {
            sink,
            stages: self.stages,
            input_buffer: Vec::with_capacity(self.max_buffer_size as usize),
            output_buffer: Vec::with_capacity(self.max_buffer_size as usize),
        }
    }
}

pub struct Pipeline<T: AudioConsumer + Send> {
    stages: Vec<Box<dyn AudioProcessor + Send>>,
    input_buffer: Vec<f32>,
    output_buffer: Vec<f32>,
    sink: T,
}

impl<T> AudioConsumer for Pipeline<T>
where
    T: AudioConsumer + Send,
{
    type Error = PipelineError;

    fn push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error> {
        let span = trace_span!("audio_pipeline.push_chunk");
        let span_guard = span.enter();

        self.input_buffer.extend_from_slice(samples);

        for stage in self.stages.iter_mut() {
            stage.process(&self.input_buffer, &mut self.output_buffer)?;
            trace!(
                input_len = self.input_buffer.len(),
                output_len = self.output_buffer.len(),
                "stage.process done"
            );

            self.input_buffer.clear();
            std::mem::swap(&mut self.input_buffer, &mut self.output_buffer);
        }

        drop(span_guard);

        self.sink
            .push_chunk(&self.input_buffer)
            .map_err(PipelineError::new)?;

        self.input_buffer.clear();
        self.output_buffer.clear();

        Ok(())
    }

    #[instrument(level = "trace", name = "audio_pipeline.finish", skip_all)]
    fn finish(&mut self) -> Result<(), Self::Error> {
        for stage in self.stages.iter_mut() {
            stage.finish()?;
        }

        self.sink.finish().map_err(PipelineError::new)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RemixerAvgError {
    #[error("Nothing to downmix")]
    NothingToDownmix,

    #[error("Only downmixing to one channel is supported")]
    Unsupported,
}

// Reduce all channels to one channel.
// Uses simple avg.
pub struct RemixerAvg {
    from_spec: AudioSpec,
    to_spec: AudioSpec,
}

impl RemixerAvg {
    pub fn new(
        from_spec: AudioSpec,
        channels: ChannelCount,
    ) -> Result<Self, RemixerAvgError> {
        if from_spec.channels < 2 {
            return Err(RemixerAvgError::NothingToDownmix);
        }

        if channels != 1 {
            return Err(RemixerAvgError::Unsupported);
        }

        let mut to_spec = from_spec;
        to_spec.channels = channels;

        Ok(Self { from_spec, to_spec })
    }

    pub fn to_channels(
        from_spec: AudioSpec,
        channels: ChannelCount,
    ) -> Result<Option<Self>, RemixerAvgError> {
        if from_spec.channels == channels {
            return Ok(None);
        }

        Self::new(from_spec, channels).map(Some)
    }
}

impl AudioProcessor for RemixerAvg {
    #[instrument(level = "trace", name = "remixer.process", skip_all)]
    fn process(
        &mut self,
        input: &[f32],
        output: &mut Vec<f32>,
    ) -> Result<(), PipelineError> {
        trace!(
            from = self.from_spec.channels,
            to = self.to_spec.channels,
            "remixing channels"
        );

        let from_channels = self.from_spec.channels;

        for els in input.chunks(from_channels as usize) {
            let avg = els.iter().sum::<f32>() / (from_channels as f32);
            output.push(avg)
        }

        Ok(())
    }

    fn output_spec(&self) -> AudioSpec {
        self.to_spec
    }

    fn finish(&mut self) -> Result<(), PipelineError> {
        Ok(())
    }
}
