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

impl From<SampleFormat> for cpal::SampleFormat {
    fn from(value: SampleFormat) -> Self {
        match value {
            SampleFormat::F32 => cpal::SampleFormat::F32,
        }
    }
}

#[derive(Debug, PartialEq)]
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
