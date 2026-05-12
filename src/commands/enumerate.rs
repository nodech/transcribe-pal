use clap::Args;

use crate::audio::device_list;

#[derive(Debug, Args)]
pub(crate) struct EnumerateCommandArgs {
    #[arg(short, long)]
    debug: bool,
}

pub(crate) fn run(cmd_args: EnumerateCommandArgs) -> Result<(), anyhow::Error> {
    let EnumerateCommandArgs { debug } = cmd_args;
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
