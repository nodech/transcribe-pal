use cpal::{
    Device, SampleFormat, SupportedBufferSize,
    traits::{DeviceTrait, HostTrait},
};

pub(crate) fn run(debug: bool) -> Result<(), anyhow::Error> {
    let def_host_id = cpal::default_host().id();

    for host_id in cpal::available_hosts() {
        let host = match cpal::host_from_id(host_id) {
            Ok(host) => host,
            Err(err) => {
                eprintln!("Failed to get host for {host_id}. Err: {err}");
                continue;
            }
        };

        let is_default = host_id == def_host_id;
        println!("{}{}", host_id, if is_default { " (Default)" } else { "" });

        let def_input_id =
            match host.default_input_device().map(|dev| dev.id()).transpose() {
                Ok(def_device) => def_device,
                Err(err) => {
                    eprintln!(
                        "{host_id} - Failed to get default device. Err: {err}"
                    );
                    None
                }
            };

        let devices = match host.input_devices() {
            Ok(devices) => devices,
            Err(err) => {
                eprintln!(
                    "{host_id} - Failed to get input devices, skipping. Err: {err}"
                );
                continue;
            }
        };

        for device in devices {
            let device_id = match device.id() {
                Ok(dev_id) => dev_id,
                Err(err) => {
                    eprintln!(
                        "{host_id} - Failed to get device id, skipping. Err: {err}"
                    );
                    continue;
                }
            };

            let def_str = match (is_default, def_input_id.as_ref()) {
                (true, Some(def_device)) if def_device == &device_id => {
                    " (Default)"
                }
                _ => "",
            };

            let desc = device.description();

            println!(
                "  {}{} - {}{}",
                device_id,
                def_str,
                if supports_mono_16k_f32(&device) {
                    "Supports - "
                } else {
                    ""
                },
                match desc {
                    Ok(desc) => desc.to_string(),
                    Err(_) => String::new(),
                },
            );

            if debug {
                enumerate_device_configs(device);
            }
        }
    }

    Ok(())
}

fn supports_mono_16k_f32(device: &Device) -> bool {
    let mut configs = match device.supported_input_configs() {
        Ok(configs) => configs,
        Err(_) => {
            return false;
        }
    };

    configs.any(|cfg| {
        cfg.sample_format() == SampleFormat::F32
            && cfg.min_sample_rate() <= 16_000
            && cfg.max_sample_rate() >= 16_000
            && cfg.channels() == 1
    })
}

fn enumerate_device_configs(device: Device) {
    let configs = match device.supported_input_configs() {
        Ok(configs) => configs,
        Err(err) => {
            eprintln!("Could not get device configs. Err: {err}");
            return;
        }
    };

    for config in configs {
        let channels = config.channels();
        let min_sample = config.min_sample_rate();
        let max_sample = config.max_sample_rate();
        let buf_size = config.buffer_size();
        let mut buffer_min = 0;
        let mut buffer_max = 0;
        let format = config.sample_format();

        if let SupportedBufferSize::Range { min, max } = buf_size {
            buffer_min = *min;
            buffer_max = *max;
        }

        println!(
            "    ch: {channels}, sample {min_sample}-{max_sample}, \
             buffer: {buffer_min}-{buffer_max}, format: {format}"
        );
    }
}
