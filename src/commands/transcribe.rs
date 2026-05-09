use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::{thread::sleep, time::Duration};

use crate::audio::AudioCallbackConsumer;
use crate::transcribe;
use crate::{audio, output};

use anyhow::Result;
use cpal::{
    InputCallbackInfo,
    traits::{DeviceTrait, StreamTrait},
};
use tracing::{debug, error};

pub(crate) fn run(
    shutdown: Arc<AtomicBool>,
    host_str: Option<String>,
    device_str: Option<String>,
) -> Result<()> {
    let host = audio::select_host(host_str)?;
    debug!("Selected host: {}", host.id());

    let device = audio::select_input_device(&host, device_str)?;
    debug!("Selected device: {:?}", device.id());

    let config = audio::find_proper_config(&device)?;
    debug!("Selected config: {:?}", config);

    debug!("Prepare outputs");
    let stdout = output::IoWriter::stdout();

    debug!("Prepare transcriber / AudioConsumer");
    let transcriber =
        transcribe::AudioTranscriberBuilder::default().build(stdout)?;

    debug!("Prepare callback adapter");
    let mut mpsc_adapter =
        audio::device_cb::MPSCAudioAdapter::new(transcriber, 100);

    // TODO: Move audio stuff out.
    let stream = {
        let mut audio_cb = mpsc_adapter.init()?;
        device.build_input_stream(
            &config,
            move |data: &[f32], _: &InputCallbackInfo| {
                // call
                if let Err(e) = audio_cb.try_push_chunk(data) {
                    error!("Failed to send data to the thread: {}.", e);
                }
            },
            |err| {
                error!("Stream received an error: {}", err);
            },
            None,
        )
    }?;

    debug!("Prepare transcriber");

    stream.play()?;

    while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
        sleep(Duration::from_millis(100));
    }

    drop(stream);
    mpsc_adapter.join()?;

    Ok(())
}
