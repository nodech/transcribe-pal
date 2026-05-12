use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::{thread::sleep, time::Duration};

use clap::Args;

use crate::audio::device::AudioDeviceBuilder;
use crate::output::MultiWriter;
use crate::transcribe;
use crate::{audio, output};

#[derive(Debug, Args)]
pub struct TranscribeCommandArgs {
    /// Audio host on the system
    #[arg(long)]
    host: Option<String>,
    /// Audio device on the host
    #[arg(long)]
    device: Option<String>,
}

pub(crate) fn run(cmd_args: TranscribeCommandArgs) -> anyhow::Result<()> {
    let TranscribeCommandArgs {
        host: host_str,
        device: device_str,
    } = cmd_args;

    let stdout = output::IoWriter::stdout();
    let multi = MultiWriter::new().push_writer(stdout);

    let transcriber =
        transcribe::AudioTranscriberBuilder::default().build(multi)?;

    let mut mpsc_adapter = audio::device_cb::MPSCAudioAdapter::new(
        transcriber,
        NonZeroUsize::try_from(100)?,
    );

    let mut device = AudioDeviceBuilder::new()
        .with_host(host_str)
        .with_device(device_str)
        .with_timeout(Some(Duration::from_secs(1)))
        .build()?;

    let audio_cb = mpsc_adapter.init()?;
    let mut stream = device.stream(audio_cb)?;

    stream.play()?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_ctrlc = shutdown.clone();

    ctrlc::set_handler(move || {
        shutdown_ctrlc.store(true, std::sync::atomic::Ordering::SeqCst);
    })?;

    while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
        sleep(Duration::from_millis(100));
    }

    drop(stream);
    mpsc_adapter.join()?;

    Ok(())
}
