use std::error::Error;
use std::{str::FromStr, time::Duration};

use anyhow::Result;
use cpal::{
    Device, DeviceId, Host, HostId, InputCallbackInfo, Stream, host_from_id,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use thiserror::Error;
use tracing::{Span, debug, debug_span, error, trace};

use crate::audio::{
    DeviceConfig, DeviceConfigError, RawBufferSize, SampleFormatError,
    device_list::is_config_supported,
};

#[derive(Debug, Error)]
pub enum AudioDeviceError {
    #[error("Failed to build stream {0}")]
    Build(cpal::Error),

    #[error("Play error {0}")]
    Play(cpal::Error),

    #[error("Pause error {0}")]
    Pause(cpal::Error),
}

pub struct AudioStream {
    inner: Stream,
}

impl AudioStream {
    pub fn play(&mut self) -> Result<(), AudioDeviceError> {
        self.inner.play().map_err(AudioDeviceError::Play)
    }

    #[allow(dead_code)]
    pub fn pause(&mut self) -> Result<(), AudioDeviceError> {
        self.inner.pause().map_err(AudioDeviceError::Pause)
    }
}

#[derive(Debug, Error)]
pub enum AudioStreamError {
    #[error(transparent)]
    Stream(#[from] cpal::Error),
}

pub trait AudioDeviceErrorCallback: Send + 'static {
    fn error(&mut self, error: AudioStreamError);
}

impl<F> AudioDeviceErrorCallback for F
where
    F: FnMut(AudioStreamError) + Send + 'static,
{
    fn error(&mut self, error: AudioStreamError) {
        self(error)
    }
}

pub trait AudioCallbackConsumer: Send + 'static {
    type Error: Error + Send + Sync + 'static;
    fn try_push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error>;
}

impl<F, E> AudioCallbackConsumer for F
where
    E: Error + Send + Sync + 'static,
    F: FnMut(&[f32]) -> Result<(), E> + Send + 'static,
{
    type Error = E;
    fn try_push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error> {
        self(samples)
    }
}

pub struct AudioDevice {
    device: Device,
    #[allow(dead_code)]
    host: Host,
    config: DeviceConfig,
    timeout: Option<Duration>,
}

impl AudioDevice {
    pub fn audio_config(&self) -> DeviceConfig {
        self.config
    }

    pub fn stream(
        &mut self,
        cb: impl AudioCallbackConsumer,
    ) -> Result<AudioStream, AudioDeviceError> {
        self.stream_with_error(cb, |_: AudioStreamError| {})
    }

    pub fn stream_with_error(
        &mut self,
        mut cb: impl AudioCallbackConsumer,
        mut err_cb: impl AudioDeviceErrorCallback,
    ) -> Result<AudioStream, AudioDeviceError> {
        let parent = Span::current();

        let span = debug_span!("audio_device.stream");
        let _guard = span.enter();

        debug!(config = ?&self.config, "building audio stream");

        let cb_span = tracing::debug_span!(
            parent: parent,
            "audio_device.audio_cb"
        );

        let channels = self.config.channels;
        let stream = self
            .device
            .build_input_stream(
                self.config.into(),
                move |data: &[f32], _: &InputCallbackInfo| {
                    let _guard = cb_span.enter();
                    trace!(
                        len = data.len(),
                        channels = channels,
                        "received audio data"
                    );

                    if let Err(e) = cb.try_push_chunk(data) {
                        error!(error = %e, "failed to process audio data");
                    }
                },
                move |err| {
                    error!(error = %err, "audio stream error");
                    err_cb.error(err.into());
                },
                self.timeout,
            )
            .map_err(AudioDeviceError::Build)?;

        debug!("built audio stream");

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
        source: cpal::Error,
    },

    #[error("No device {device} for host {host}")]
    NoDeviceForHost { host: String, device: String },

    #[error("Supported config error: {0}")]
    SupportedConfigError(cpal::Error),

    #[error("Default config error: {0}")]
    DefaultConfigError(cpal::Error),

    #[error(transparent)]
    UnsupportedFormat(#[from] SampleFormatError),

    #[error(transparent)]
    UnsupportedDeviceConfig(#[from] DeviceConfigError),
}

#[derive(Default)]
pub struct AudioDeviceBuilder {
    host_str: Option<String>,
    device_str: Option<String>,
    config: DeviceConfig,
    timeout: Option<Duration>,
    buffer_size: Option<usize>,
}

impl AudioDeviceBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(mut self, config: DeviceConfig) -> Self {
        self.config = config;
        self
    }

    // TODO: Maybe use Option<&str>?
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

    /// Buffer size per channel
    pub fn with_buffer_size(mut self, buffer_size: Option<usize>) -> Self {
        self.buffer_size = buffer_size;
        self
    }

    pub fn build(self) -> Result<AudioDevice, AudioDeviceBuilderError> {
        let host = self.resolve_host()?;
        let device = self.resolve_device(&host)?;
        let config = self.resolve_config(&device)?;

        Ok(AudioDevice { host, device, config, timeout: self.timeout })
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
        // buffer_size: usize,
    ) -> Result<DeviceConfig, AudioDeviceBuilderError> {
        let mut configs = device
            .supported_input_configs()
            .map_err(AudioDeviceBuilderError::SupportedConfigError)?;

        let buffer_size = self.buffer_size.unwrap_or(2048) as RawBufferSize;

        if let Some(cfg) =
            configs.find(|cfg| is_config_supported(cfg, &self.config))
        {
            let supported = cfg.with_sample_rate(self.config.sample_rate);
            let device_config =
                DeviceConfig::try_from_stream_config(supported, buffer_size)?;

            return Ok(device_config);
        }

        let default = device
            .default_input_config()
            .map_err(AudioDeviceBuilderError::DefaultConfigError)?;

        // This line also ensures format is supported.
        // TODO: Maybe try finding next best thing here if this fails?
        let device_config =
            DeviceConfig::try_from_stream_config(default, buffer_size)?;

        Ok(device_config)
    }
}
