use crate::audio::device_list;

pub(crate) fn run(debug: bool) -> Result<(), anyhow::Error> {
    let hosts = device_list::list_hosts();

    for host in hosts {
        println!("{}", host.summary());

        let devices = match host.list_devices() {
            Ok(devices) => devices,
            Err(err) => {
                eprintln!("    {}", err);
                continue;
            }
        };

        for device in devices {
            println!("    {}", device.summary());

            if debug {
                let configs = match device.list_configs() {
                    Ok(configs) => configs,
                    Err(err) => {
                        eprintln!("    {}", err);
                        continue;
                    }
                };

                for config in configs {
                    println!("    {}", config.summary());
                }
            }
        }
    }

    Ok(())
}
