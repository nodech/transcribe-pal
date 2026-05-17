use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::{thread::sleep, time::Duration};

use clap::{Args, ValueEnum};

use crate::audio::device::AudioDeviceBuilder;
use crate::output::MultiWriter;
use crate::transcribe::{self, ModelConfig};
use crate::{audio, output};

#[derive(Debug, Args)]
pub(crate) struct TranscribeCommandArgs {
    /// Audio host on the system
    #[arg(long)]
    host: Option<String>,
    /// Audio device on the host
    #[arg(long)]
    device: Option<String>,

    /// Do not print to stdout
    #[arg(long)]
    no_stdout: bool,

    /// Path to extracted model
    #[arg(long)]
    model_path: Option<PathBuf>,

    /// Model type
    #[arg(long, value_enum)]
    model_kind: Option<CommandModelKind>,

    /// Microphone threshold (0.0 - 1.0)
    #[arg(long)]
    mic_threshold: Option<f32>,

    /// Speech end delay (In milliseconds)
    #[arg(long, value_parser = clap::value_parser!(u64).range(150..=1800))]
    speech_end_delay: Option<u64>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CommandModelKind {
    /// This does not support direct streaming, but can "stream" with VAD.
    Parakeet,
}

impl From<CommandModelKind> for transcribe::ModelKind {
    fn from(val: CommandModelKind) -> Self {
        match val {
            CommandModelKind::Parakeet => transcribe::ModelKind::Parakeet,
        }
    }
}

pub(crate) fn run(cmd_args: TranscribeCommandArgs) -> anyhow::Result<()> {
    let TranscribeCommandArgs {
        host,
        device,
        no_stdout,
        model_path,
        model_kind,
        mic_threshold,
        speech_end_delay: speech_delay,
    } = cmd_args;

    let mut multi = MultiWriter::new();

    if !no_stdout {
        multi.push_writer(output::IoWriter::stdout());
    }

    if multi.is_empty() {
        return Err(anyhow::anyhow!("no transcript output is configured"));
    }

    let model = ModelConfig::default()
        .with_path_opt(model_path)
        .with_kind_opt(model_kind);

    let transcriber = transcribe::AudioTranscriberBuilder::default()
        .try_with_mic_threshold_opt(mic_threshold)?
        .try_with_speech_end_delay_opt(speech_delay.map(Duration::from_millis))?
        .with_model(model)
        .build(multi)?;

    let mut mpsc_adapter = MPSCAudioAdapter::new(NonZeroUsize::try_from(100)?);

    let mut device = AudioDeviceBuilder::new()
        .with_host(host)
        .with_device(device)
        .with_timeout(Some(Duration::from_secs(1)))
        .build()?;

    let audio_cb = mpsc_adapter.init(transcriber)?;
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
