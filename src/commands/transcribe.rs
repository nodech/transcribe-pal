use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::{io, thread};
use std::{sync::mpsc, thread::sleep, time::Duration};

use crate::audio;
use crate::transcribe;

use anyhow::Result;
use cpal::{
    InputCallbackInfo,
    traits::{DeviceTrait, StreamTrait},
};
use tracing::{debug, error};
use transcribe_rs::onnx::parakeet::ParakeetModel;
use transcribe_rs::transcriber::{Transcriber, VadChunked};

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

    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(32);

    let model = transcribe::setup_model()?;
    debug!("Model is setup.");

    let chunked = transcribe::chunked_transcriber()?;
    debug!("Vad is setup.");

    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &InputCallbackInfo| {
            if let Err(e) = tx.try_send(data.to_vec()) {
                error!("Failed to send data to the thread: {}.", e);
            }
        },
        |err| {
            error!("Stream received an error: {}", err);
        },
        None,
    )?;

    debug!("Stream is setup.");

    debug!("Spawning model thread.");
    let transcribe_thread = thread::spawn(move || {
        transcribe_worker(model, chunked, rx);
    });

    stream.play()?;

    while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
        sleep(Duration::from_millis(100));
    }

    drop(stream);

    transcribe_thread.join().unwrap();

    Ok(())
}

fn transcribe_worker(
    mut model: ParakeetModel,
    mut chunked: VadChunked,
    rx: mpsc::Receiver<Vec<f32>>,
) {
    let mut skip_segments = 0;
    let mut stdout = io::stdout();

    while let Ok(samples) = rx.recv() {
        let results = match chunked.feed(&mut model, &samples) {
            Ok(res) => res,
            Err(e) => {
                error!("Failed to feed samples to the model: {}", e);
                continue;
            }
        };

        for result in results {
            print!("{} ", result.text);

            if let Some(segments) = result.segments {
                skip_segments += segments.len();
            }
        }

        _ = stdout.flush();
    }

    match chunked.finish(&mut model) {
        Ok(finished) => {
            if let Some(segments) = finished.segments {
                for segment in segments.iter().skip(skip_segments) {
                    print!("{}", segment.text);
                }
            }
        }
        Err(err) => {
            error!("Failed to finish the transcription: {}.", err);
        }
    }

    println!();
}
