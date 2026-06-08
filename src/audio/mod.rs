use std::error::Error;

use cpal::{BufferSize, StreamConfig, SupportedStreamConfig};

pub mod device;
pub mod device_cb;
pub mod device_list;
pub mod pipeline;

pub type SampleRate = cpal::SampleRate;
pub type ChannelCount = cpal::ChannelCount;
pub type RawBufferSize = cpal::FrameCount;

pub trait AudioConsumer {
    type Error: Error + Send + Sync + 'static;

    fn push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error>;
    fn finish(&mut self) -> Result<(), Self::Error>;
}

pub trait AudioCallbackConsumer: Send + 'static {
    type Error: Error + Send + Sync + 'static;
    fn try_push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error>;
}

#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum SampleFormat {
    #[default]
    F32,
}

#[derive(Debug, thiserror::Error)]
pub enum SampleFormatError {
    #[error("Unsupported sample format: {0}")]
    Unsupported(cpal::SampleFormat),
}

impl From<SampleFormat> for cpal::SampleFormat {
    fn from(value: SampleFormat) -> Self {
        match value {
            SampleFormat::F32 => cpal::SampleFormat::F32,
        }
    }
}

impl TryFrom<cpal::SampleFormat> for SampleFormat {
    type Error = SampleFormatError;

    fn try_from(value: cpal::SampleFormat) -> Result<Self, Self::Error> {
        Ok(match value {
            cpal::SampleFormat::F32 => SampleFormat::F32,
            _ => return Err(SampleFormatError::Unsupported(value)),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeviceConfigError {
    #[error(transparent)]
    SampleFormatError(#[from] SampleFormatError),

    #[error("Unknown buffer size")]
    UnknownBufferSize,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct DeviceConfig {
    pub format: SampleFormat,
    pub sample_rate: SampleRate,
    pub channels: ChannelCount,
    pub frames_per_buffer: RawBufferSize,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
            format: SampleFormat::F32,
            frames_per_buffer: 4096,
        }
    }
}

impl DeviceConfig {
    pub fn try_from_stream_config(
        stream_config: SupportedStreamConfig,
        target_buf_size: RawBufferSize,
    ) -> Result<Self, DeviceConfigError> {
        let buf_size = match stream_config.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => {
                target_buf_size.clamp(*min, *max)
            }
            cpal::SupportedBufferSize::Unknown => {
                // NOTE: Maybe we can work with the unknown sizes in the future.
                // We deny for now.
                return Err(DeviceConfigError::UnknownBufferSize);
            }
        };

        Ok(Self {
            channels: stream_config.channels(),
            sample_rate: stream_config.sample_rate(),
            format: stream_config.sample_format().try_into()?,
            frames_per_buffer: buf_size,
        })
    }
}

impl From<DeviceConfig> for StreamConfig {
    fn from(value: DeviceConfig) -> Self {
        Self {
            channels: value.channels,
            sample_rate: value.sample_rate,
            buffer_size: BufferSize::Fixed(value.frames_per_buffer),
        }
    }
}
