use std::error::Error;

pub mod device;
pub mod device_cb;
pub mod device_list;

pub trait AudioConsumer {
    type Error: Error + Send + Sync + 'static;

    fn push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error>;
    fn finish(&mut self) -> Result<(), Self::Error>;
}

pub trait AudioCallbackConsumer: Send + 'static {
    fn try_push_chunk(&mut self, samples: &[f32]) -> anyhow::Result<()>;
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

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct DeviceConfig {
    pub format: SampleFormat,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
            format: SampleFormat::F32,
        }
    }
}
