use std::{str::FromStr, time::Duration};

use anyhow::Result;
use cpal::{
    DefaultStreamConfigError, Device, DeviceId, DeviceIdError, Host, HostId,
    InputCallbackInfo, Stream, StreamConfig, SupportedStreamConfigsError,
    host_from_id,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use thiserror::Error;

use crate::audio::{
    AudioCallbackConsumer, DeviceConfig, SampleFormat, SampleFormatError,
    device_list::is_config_supported,
};

#[derive(Debug, Error)]
pub enum AudioDeviceError {
    #[error("Failed to build stream {0}")]
    Build(#[from] cpal::BuildStreamError),

    #[error("Play error {0}")]
    Play(#[from] cpal::PlayStreamError),

    #[error("Pause error {0}")]
    Pause(#[from] cpal::PauseStreamError),
}

pub struct AudioStream {
    inner: Stream,
}

impl AudioStream {
    pub fn play(&mut self) -> Result<(), AudioDeviceError> {
        self.inner.play().map_err(Into::into)
    }

    #[allow(dead_code)]
    pub fn pause(&mut self) -> Result<(), AudioDeviceError> {
        self.inner.pause().map_err(Into::into)
    }
}

pub struct AudioDevice {
    device: Device,
    #[allow(dead_code)]
    host: Host,
    stream_config: StreamConfig,
    config: DeviceConfig,
    timeout: Option<Duration>,
}

impl AudioDevice {
    pub fn audio_config(&self) -> DeviceConfig {
        self.config
    }

    pub fn stream(
        &mut self,
        mut cb: impl AudioCallbackConsumer,
    ) -> Result<AudioStream, AudioDeviceError> {
        let stream = self.device.build_input_stream(
            &self.stream_config,
            move |data: &[f32], _: &InputCallbackInfo| {
                if let Err(e) = cb.try_push_chunk(data) {
                    tracing::error!("Failed to process data: {}", e);
                }
            },
            |err| {
                tracing::error!("Stream received an error: {}", err);
            },
            self.timeout,
        )?;

        Ok(AudioStream { inner: stream })
    }
}

#[derive(Debug, Error)]
pub enum AudioDeviceBuilderError {
    #[error(r#"Host "{0}" unavailable"#)]
    HostUnavailable(String),

    #[error("Default device not found for {host}")]
    DefaultInputDeviceNotFound { host: String },

    #[error("Invalid device id {device}, {source}")]
    InvalidDeviceID {
        device: String,
        #[source]
        source: DeviceIdError,
    },

    #[error("No device {device} for host {host}")]
    NoDeviceForHost { host: String, device: String },

    #[error("Supported config error {0}")]
    SupportedConfigError(#[from] SupportedStreamConfigsError),

    #[error("Default config error {0}")]
    DefaultConfigError(#[from] DefaultStreamConfigError),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(#[from] SampleFormatError),
}

#[derive(Default)]
pub struct AudioDeviceBuilder {
    host_str: Option<String>,
    device_str: Option<String>,
    config: DeviceConfig,
    timeout: Option<Duration>,
}

impl AudioDeviceBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_host(mut self, host_str: Option<String>) -> Self {
        self.host_str = host_str;
        self
    }

    pub fn with_device(mut self, device_str: Option<String>) -> Self {
        self.device_str = device_str;
        self
    }

    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn build(self) -> Result<AudioDevice, AudioDeviceBuilderError> {
        let host = self.resolve_host()?;
        let device = self.resolve_device(&host)?;
        let (stream_config, config) = self.resolve_config(&device)?;

        Ok(AudioDevice {
            host,
            device,
            stream_config,
            config,
            timeout: self.timeout,
        })
    }

    fn resolve_host(&self) -> Result<Host, AudioDeviceBuilderError> {
        let host = match self.host_str.as_ref() {
            Some(host_str) => {
                let host_id = HostId::from_str(host_str).map_err(|_| {
                    AudioDeviceBuilderError::HostUnavailable(host_str.clone())
                })?;

                host_from_id(host_id).map_err(|_| {
                    AudioDeviceBuilderError::HostUnavailable(host_str.clone())
                })?
            }
            None => cpal::default_host(),
        };

        Ok(host)
    }

    fn resolve_device(
        &self,
        host: &Host,
    ) -> Result<Device, AudioDeviceBuilderError> {
        let device = match self.device_str.as_ref() {
            Some(device_str) => {
                let device_id =
                    DeviceId::from_str(device_str).map_err(|e| {
                        AudioDeviceBuilderError::InvalidDeviceID {
                            device: device_str.clone(),
                            source: e,
                        }
                    })?;

                host.device_by_id(&device_id).ok_or_else(|| {
                    AudioDeviceBuilderError::NoDeviceForHost {
                        host: host.id().to_string(),
                        device: device_id.to_string(),
                    }
                })?
            }
            None => host.default_input_device().ok_or_else(|| {
                AudioDeviceBuilderError::DefaultInputDeviceNotFound {
                    host: host.id().to_string(),
                }
            })?,
        };

        Ok(device)
    }

    fn resolve_config(
        &self,
        device: &Device,
    ) -> Result<(StreamConfig, DeviceConfig), AudioDeviceBuilderError> {
        let mut configs = device.supported_input_configs()?;

        Ok(
            match configs.find(|cfg| is_config_supported(cfg, &self.config)) {
                Some(cfg) => {
                    let sample_format: SampleFormat =
                        cfg.sample_format().try_into()?;

                    let fin_cfg =
                        cfg.with_sample_rate(self.config.sample_rate).config();

                    let sample_rate = fin_cfg.sample_rate;
                    let channels = fin_cfg.channels;

                    (
                        fin_cfg,
                        DeviceConfig {
                            channels,
                            sample_rate,
                            format: sample_format,
                        },
                    )
                }
                None => {
                    let def = device.default_input_config()?;
                    let sample_format: SampleFormat =
                        def.sample_format().try_into()?;
                    let sample_rate = def.sample_rate();
                    let channels = def.channels();

                    (
                        def.config(),
                        DeviceConfig {
                            channels,
                            sample_rate,
                            format: sample_format,
                        },
                    )
                }
            },
        )
    }
}
