use std::error::Error;

pub mod device;
pub mod device_cb;
pub mod device_list;

use cpal::SampleFormat;

pub trait AudioConsumer {
    type Error: Error + Send + Sync + 'static;

    fn push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error>;
    fn finish(&mut self) -> Result<(), Self::Error>;
}

pub trait AudioCallbackConsumer: Send + 'static {
    fn try_push_chunk(&mut self, samples: &[f32]) -> anyhow::Result<()>;
}

#[derive(Debug, PartialEq)]
pub struct AudioDeviceConfig {
    format: SampleFormat,
    sample_rate: u32,
    channels: u16,
}

impl Default for AudioDeviceConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
            format: SampleFormat::F32,
        }
    }
}
