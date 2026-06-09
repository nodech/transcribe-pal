//! Audio resampler for the audio pipeline.

use rubato::{
    ResampleError, Resampler,
    audioadapter_buffers::{SizeError, direct::SequentialSlice},
};
use tracing::{instrument, trace};

use super::super::{RawBufferSize, SampleRate};
use super::{AudioProcessor, AudioSpec, PipelineError};

#[derive(Debug, thiserror::Error)]
pub enum ResampleProcessorError {
    #[error("There must only be 1 channel")]
    IncorrectChannels,

    #[error("Resampler error: {0}")]
    ResamplerConstructor(#[from] rubato::ResamplerConstructionError),

    #[error("Input slice error: {0}")]
    InputSliceError(SizeError),

    #[error("Output slice error: {0}")]
    OutputSliceError(SizeError),

    #[error("Resample failed: {0}")]
    ResampleError(ResampleError),
}

// Move the resampler into thread.
pub struct ResampleProcessor {
    #[allow(dead_code)]
    from_spec: AudioSpec,
    to_spec: AudioSpec,
    resampler: rubato::Fft<f32>,
}

impl ResampleProcessor {
    pub fn new(
        from: AudioSpec,
        to_sample_rate: SampleRate,
    ) -> Result<Self, ResampleProcessorError> {
        if from.channels != 1 {
            return Err(ResampleProcessorError::IncorrectChannels);
        }

        // This and frames_per_buffer MUST be equal
        // given the channels count is 1.
        let input_buffer_size = from.samples_per_buffer();

        let resampler = rubato::Fft::<f32>::new(
            from.sample_rate as usize,
            to_sample_rate as usize,
            input_buffer_size as usize,
            1,
            from.channels as usize,
            rubato::FixedSync::Input,
        )?;

        let mut to_spec = from;
        to_spec.sample_rate = to_sample_rate;
        to_spec.frames_per_buffer =
            resampler.output_frames_max() as RawBufferSize;

        Ok(Self {
            from_spec: from,
            to_spec,
            resampler,
        })
    }

    pub fn to_sample_rate(
        from_spec: AudioSpec,
        to_sample_rate: SampleRate,
    ) -> Result<Option<Self>, ResampleProcessorError> {
        if from_spec.sample_rate == to_sample_rate {
            return Ok(None);
        }

        Self::new(from_spec, to_sample_rate).map(Some)
    }
}

impl AudioProcessor for ResampleProcessor {
    fn output_spec(&self) -> AudioSpec {
        self.to_spec
    }

    #[instrument(level = "trace", name = "resample.process", skip_all)]
    fn process(
        &mut self,
        input: &[f32],
        output: &mut Vec<f32>,
    ) -> Result<(), PipelineError> {
        let input_slice = SequentialSlice::new(input, 1, input.len())
            .map_err(ResampleProcessorError::InputSliceError)
            .map_err(PipelineError::new)?;

        let output_size = self.to_spec.frames_per_buffer as usize;
        output.resize(output_size, 0.0f32);

        let mut output_slice =
            SequentialSlice::new_mut(output.as_mut_slice(), 1, output_size)
                .map_err(ResampleProcessorError::OutputSliceError)
                .map_err(PipelineError::new)?;

        let (frames_in, frames_out) = self
            .resampler
            .process_into_buffer(&input_slice, &mut output_slice, None)
            .map_err(ResampleProcessorError::ResampleError)
            .map_err(PipelineError::new)?;

        trace!(frames_in = frames_in, frames_out = frames_out, "resampled");

        output.resize(frames_out, 0.0f32);

        Ok(())
    }

    fn finish(&mut self) -> Result<(), PipelineError> {
        Ok(())
    }
}
