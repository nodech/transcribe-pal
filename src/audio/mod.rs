use std::str::FromStr;

pub mod device;

use cpal::{
    Device, DeviceId, Host, HostId, SampleFormat, SampleRate, StreamConfig,
    host_from_id,
    traits::{DeviceTrait, HostTrait},
};

use anyhow::{Context, Result, anyhow};

pub fn select_host(host_str: Option<String>) -> Result<Host> {
    if let Some(name) = host_str {
        let host_id = HostId::from_str(&name)
            .with_context(|| format!("Failed to parse Host ID: {}", &name))?;

        return host_from_id(host_id).with_context(|| {
            format!("Failed to get host by Host ID: {}", &host_id)
        });
    }

    Ok(cpal::default_host())
}

pub fn select_input_device(
    host: &Host,
    device_str: Option<String>,
) -> Result<Device> {
    if let Some(name) = device_str {
        let device_id = DeviceId::from_str(&name)
            .with_context(|| format!("Failed to parse Device ID: {}", &name))?;

        let device = host.device_by_id(&device_id);

        return device.ok_or_else(|| {
            anyhow!("Failed to find device: {} for {}", device_id, host.id())
        });
    }

    host.default_input_device().ok_or_else(|| {
        anyhow!("Failed to get default input device for {}", host.id())
    })
}

pub fn find_proper_config(device: &Device) -> Result<StreamConfig> {
    let mut configs = device
        .supported_input_configs()
        .with_context(|| "Failed to get supported input configs.")?;

    let cfg = configs
        .find(|cfg| {
            cfg.sample_format() == SampleFormat::F32
                && cfg.min_sample_rate() <= 16_000
                && cfg.max_sample_rate() >= 16_000
                && cfg.channels() == 1
        })
        .ok_or_else(|| {
            anyhow!("Could not find proper config for the input.")
        })?;

    let sample_rate: SampleRate = 16_000;
    Ok(cfg.with_sample_rate(sample_rate).config())
}
