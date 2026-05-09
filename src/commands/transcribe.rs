use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::{thread::sleep, time::Duration};

use crate::audio::device::AudioDeviceBuilder;
use crate::transcribe;
use crate::{audio, output};

pub(crate) fn run(
    shutdown: Arc<AtomicBool>,
    host_str: Option<String>,
    device_str: Option<String>,
) -> anyhow::Result<()> {
    let mut device = AudioDeviceBuilder::new()
        .with_host(host_str)
        .with_device(device_str)
        .with_timeout(Some(Duration::from_secs(1)))
        .build()?;

    let stdout = output::IoWriter::stdout();

    let transcriber =
        transcribe::AudioTranscriberBuilder::default().build(stdout)?;

    let mut mpsc_adapter =
        audio::device_cb::MPSCAudioAdapter::new(transcriber, 100);

    let audio_cb = mpsc_adapter.init()?;
    let mut stream = device.stream(audio_cb)?;

    stream.play()?;

    while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
        sleep(Duration::from_millis(100));
    }

    drop(stream);
    mpsc_adapter.join()?;

    Ok(())
}
