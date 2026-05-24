use cpal::{
    Device, Host, SupportedStreamConfigRange,
    traits::{DeviceTrait, HostTrait},
};
use std::fmt;
use thiserror::Error;

use crate::audio::{ChannelCount, DeviceConfig, SampleFormat, SampleRate};

#[derive(Debug, Default)]
pub struct HostSummary {
    pub name: String,
    pub error: Option<String>,
    pub is_default: bool,
}

impl fmt::Display for HostSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;

        if self.is_default {
            write!(f, " (DEFAULT)")?;
        }

        if let Some(err) = &self.error {
            write!(f, " [error: {}]", err)?;
        }

        Ok(())
    }
}

pub struct ListedHost {
    host: Option<Host>,
    summary: HostSummary,
}

#[derive(Debug, Error)]
pub enum HostListDevicesError {
    #[error("No host")]
    NoHost,
    #[error("Failed to get input devices: {0}")]
    DevicesError(#[from] cpal::DevicesError),
}

impl ListedHost {
    pub fn summary(&self) -> &HostSummary {
        &self.summary
    }

    pub fn list_devices(
        &self,
    ) -> Result<Vec<ListedDevice>, HostListDevicesError> {
        let Some(host) = &self.host else {
            return Err(HostListDevicesError::NoHost);
        };

        let mut devices = vec![];
        let def_dev_id = host.default_input_device().and_then(|d| d.id().ok());

        for device in host.input_devices()? {
            let device_id = match device.id() {
                Ok(id) => id,
                Err(err) => {
                    devices.push(ListedDevice {
                        device: Some(device),
                        summary: DeviceSummary {
                            error: Some(err.to_string()),
                            ..Default::default()
                        },
                    });
                    continue;
                }
            };

            // Don't show default for every host, just the default
            // host/device pair.
            let is_default = self.summary.is_default
                && def_dev_id
                    .as_ref()
                    .map(|id| id == &device_id)
                    .unwrap_or(false);

            let description = device.description().ok().map(|f| f.to_string());
            let supports = device_supports_config(&device);

            devices.push(ListedDevice {
                device: Some(device),
                summary: DeviceSummary {
                    name: Some(device_id.to_string()),
                    error: None,
                    description,
                    supports,
                    is_default,
                },
            })
        }

        Ok(devices)
    }
}

pub fn list_hosts() -> Vec<ListedHost> {
    let def_host_id = cpal::default_host().id();
    let available = cpal::available_hosts();
    let mut hosts = vec![];

    for host_id in available {
        let is_default = host_id == def_host_id;
        let host = cpal::host_from_id(host_id);

        hosts.push(match host {
            Ok(host) => ListedHost {
                host: Some(host),
                summary: HostSummary {
                    name: host_id.to_string(),
                    is_default,
                    error: None,
                },
            },
            Err(err) => ListedHost {
                host: None,
                summary: HostSummary {
                    name: host_id.to_string(),
                    error: Some(err.to_string()),
                    is_default,
                },
            },
        });
    }

    hosts
}

#[derive(Debug, Default)]
pub struct DeviceSummary {
    pub name: Option<String>,
    pub description: Option<String>,
    pub error: Option<String>,
    pub supports: bool,
    pub is_default: bool,
}

impl fmt::Display for DeviceSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name.as_deref().unwrap_or("-"))?;

        if self.is_default {
            write!(f, " (DEFAULT)")?;
        }

        if self.supports {
            write!(f, " (SUPPORTS)")?;
        }

        if let Some(err) = &self.error {
            write!(f, " [error: {}]", err)?;
        }

        if let Some(desc) = &self.description {
            write!(f, " - {}", desc)?;
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum DeviceConfigListError {
    #[error("No device")]
    NoDevice,
    #[error("Supported device config error: {0}")]
    SupportedDeviceConfigError(#[from] cpal::SupportedStreamConfigsError),
}

pub struct ListedDevice {
    device: Option<Device>,
    summary: DeviceSummary,
}

impl ListedDevice {
    pub fn summary(&self) -> &DeviceSummary {
        &self.summary
    }

    pub fn list_configs(
        &self,
    ) -> Result<Vec<ListedDeviceConfig>, DeviceConfigListError> {
        let Some(device) = &self.device else {
            return Err(DeviceConfigListError::NoDevice);
        };

        let configs = device.supported_input_configs()?;

        Ok(configs
            .map(|config| ListedDeviceConfig {
                // config,
                summary: DeviceConfigSummary {
                    format: config.sample_format().to_string().to_uppercase(),
                    channels: config.channels(),
                    min_sample: config.min_sample_rate(),
                    max_sample: config.max_sample_rate(),
                    buffer: (*config.buffer_size()).into(),
                },
            })
            .collect())
    }
}

#[derive(Debug)]
pub struct ListedDeviceConfig {
    // config: SupportedStreamConfigRange,
    summary: DeviceConfigSummary,
}

impl ListedDeviceConfig {
    pub fn summary(&self) -> &DeviceConfigSummary {
        &self.summary
    }
}

#[derive(Debug)]
pub struct DeviceConfigBufferSummary(cpal::SupportedBufferSize);

impl From<cpal::SupportedBufferSize> for DeviceConfigBufferSummary {
    fn from(value: cpal::SupportedBufferSize) -> Self {
        Self(value)
    }
}

impl Default for DeviceConfigBufferSummary {
    fn default() -> Self {
        DeviceConfigBufferSummary(cpal::SupportedBufferSize::Unknown)
    }
}

impl fmt::Display for DeviceConfigBufferSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            cpal::SupportedBufferSize::Range { min, max } => {
                write!(f, "{min}-{max}")
            }
            cpal::SupportedBufferSize::Unknown => {
                write!(f, "unknown")
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct DeviceConfigSummary {
    pub format: String,
    pub channels: ChannelCount,
    pub min_sample: SampleRate,
    pub max_sample: SampleRate,
    pub buffer: DeviceConfigBufferSummary,
}

impl fmt::Display for DeviceConfigSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ch: {}", self.channels)?;
        write!(f, ", sample: {}-{}", self.min_sample, self.max_sample)?;
        write!(f, ", buffer: {}", self.buffer)?;
        write!(f, ", format: {}", self.format)?;

        Ok(())
    }
}

fn device_supports_config(device: &Device) -> bool {
    let Ok(mut configs) = device.supported_input_configs() else {
        return false;
    };

    configs.any(|cfg| {
        is_config_supported(
            &cfg,
            &DeviceConfig {
                channels: 1,
                format: SampleFormat::F32,
                sample_rate: 16_000,
            },
        )
    })
}

pub fn is_config_supported(
    config: &SupportedStreamConfigRange,
    cmp_to: &DeviceConfig,
) -> bool {
    config.sample_format() == cmp_to.format.into()
        && config.min_sample_rate() <= cmp_to.sample_rate
        && config.max_sample_rate() >= cmp_to.sample_rate
        && config.channels() == cmp_to.channels
}
