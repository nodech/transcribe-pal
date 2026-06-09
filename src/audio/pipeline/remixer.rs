//! Audio remixer for the audio pipeline.
//! Downmixes to 1 channel using avg.

use tracing::{instrument, trace};

use super::super::ChannelCount;
use super::{AudioProcessor, AudioSpec, PipelineError};

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
