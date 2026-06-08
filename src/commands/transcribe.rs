use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::{thread::sleep, time::Duration};

use clap::{Args, ValueEnum};
use tracing::{debug, instrument};

use crate::audio::device::AudioDeviceBuilder;
use crate::audio::device_cb::MPSCAudioAdapter;
use crate::audio::pipeline::{PipelineBuilder, PipelineError, RemixerAvg};
#[cfg(feature = "wayland")]
use crate::output::WTypeWriter;
use crate::output::{IoWriter, MultiWriter};
use crate::resample::ResampleProcessor;
use crate::transcribe::{self, ModelConfig};

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

    #[cfg(feature = "wayland")]
    /// Print text using wtype
    #[arg(long)]
    wtype: bool,

    /// Path to extracted model
    #[arg(long)]
    model_path: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = CommandModelKind::Parakeet)]
    model_kind: CommandModelKind,

    /// Microphone threshold (0.0 - 1.0)
    #[arg(long)]
    mic_threshold: Option<f32>,

    /// Speech end delay (In milliseconds 150 - 1800)
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

#[instrument(level = "info", name = "transcribe", skip_all)]
pub(crate) fn run(cmd_args: TranscribeCommandArgs) -> anyhow::Result<()> {
    debug!(command_args = ?cmd_args, "transcribe");

    let TranscribeCommandArgs {
        host,
        device,
        no_stdout,
        #[cfg(feature = "wayland")]
        wtype,
        model_path,
        model_kind,
        mic_threshold,
        speech_end_delay: speech_delay,
    } = cmd_args;

    let mut multi = MultiWriter::new();

    if !no_stdout {
        debug!("enabled stdout writer");
        multi.push_writer(IoWriter::stdout());
    }

    #[cfg(feature = "wayland")]
    if wtype {
        debug!("enabled wtype writer");
        multi.push_writer(WTypeWriter::new());
    }

    if multi.is_empty() {
        return Err(anyhow::anyhow!("no transcript output is configured"));
    }

    let model = ModelConfig::default()
        .with_path_opt(model_path)
        .with_kind(model_kind);
    let model_config = model.audio_conig();

    debug!(model = ?model, model_audio_config = ?model_config,
        "model configs");

    let transcriber = transcribe::AudioTranscriberBuilder::default()
        .try_with_mic_threshold_opt(mic_threshold)?
        .try_with_speech_end_delay_opt(speech_delay.map(Duration::from_millis))?
        .with_model(model)
        .build(multi)?;

    debug!("built transcriber");

    let mpsc_adapter = MPSCAudioAdapter::new(NonZeroUsize::try_from(100)?);

    let mut device = AudioDeviceBuilder::new()
        .with_host(host)
        .with_device(device)
        .with_config(model_config)
        .with_buffer_size(Some(2048))
        .with_timeout(Some(Duration::from_secs(1)))
        .build()?;

    debug!("built audio device");

    debug!("building audio pipeline");
    let audio_config = device.audio_config();

    if audio_config.format != model_config.format {
        return Err(anyhow::anyhow!(
            "unsupported format: {:?}, expected: {:?}",
            audio_config.format,
            model_config.format
        ));
    }

    let audio_pipeline = PipelineBuilder::new(audio_config)
        .with_stage(|spec| {
            RemixerAvg::to_channels(spec, model_config.channels)
                .map_err(PipelineError::new)
        })?
        .with_stage(|spec| {
            ResampleProcessor::to_sample_rate(spec, model_config.sample_rate)
                .map_err(PipelineError::new)
        })?
        .build(transcriber);

    debug!("built audio pipeline");

    let (adapter_handle, audio_cb) = mpsc_adapter.spawn(audio_pipeline)?;

    let mut stream = device.stream(audio_cb)?;

    debug!("setup ctrl-c handler");
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_ctrlc = shutdown.clone();

    ctrlc::set_handler(move || {
        shutdown_ctrlc.store(true, std::sync::atomic::Ordering::SeqCst);
    })?;

    debug!("starting audio stream");
    stream.play()?;

    while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
        sleep(Duration::from_millis(100));
    }

    drop(stream);
    debug!("joining adapter thread");
    adapter_handle.join()?;

    debug!("done");

    Ok(())
}
