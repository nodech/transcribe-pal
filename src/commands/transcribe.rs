use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::anyhow;
use clap::Args;
use tracing::{debug, instrument};

use crate::audio::{
    device::AudioDeviceBuilder,
    device_cb::MPSCAudioAdapter,
    pipeline::{PipelineBuilder, RemixerAvg, ResampleProcessor},
};

use crate::commands::{
    CommandModelKind,
    common::{MultiWriterOptions, multi_writer},
};

use crate::model::{self, StoreDirectoryPath};
use crate::shutdown::Shutdown;
use crate::transcribe::{
    self, MAX_SPEECH_END_DELAY_MS, MIN_SPEECH_END_DELAY_MS, ModelConfig,
    ModelKind,
};

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

    /// Path to the store
    #[arg(long)]
    store_dir: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = CommandModelKind::Parakeet)]
    model: CommandModelKind,

    /// Microphone threshold (0.0 - 1.0)
    #[arg(long)]
    mic_threshold: Option<f32>,

    #[arg(
        long,
        help = format!("Speech end delay in milliseconds ({MIN_SPEECH_END_DELAY_MS} - {MAX_SPEECH_END_DELAY_MS})"),
        value_parser = clap::value_parser!(u64).range(
            MIN_SPEECH_END_DELAY_MS..=MAX_SPEECH_END_DELAY_MS)
    )]
    speech_end_delay: Option<u64>,
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
        store_dir,
        model,
        mic_threshold,
        speech_end_delay: speech_delay,
    } = cmd_args;

    let model_kind = ModelKind::from(model);

    let store_dir = StoreDirectoryPath::from_opt_path(store_dir)?;
    let model_manifests = model::load_manifests()?;
    let manifest = model_manifests.get(model_kind.as_str()).ok_or(anyhow!(
        "Model {} is not supported (No manifest).",
        model_kind.as_str()
    ))?;

    let fs_backend = model::FSBackend::new();
    let mut store = model::Store::new(store_dir, fs_backend);
    let model_store = model::ModelStore::from_store(&mut store, manifest);

    let multi = multi_writer(MultiWriterOptions {
        stdout: !no_stdout,
        #[cfg(feature = "wayland")]
        wtype,
    })?;

    let model = ModelConfig::default()
        .with_path_opt(Some(model_store.model_path()))
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
        })?
        .with_stage(|spec| {
            ResampleProcessor::to_sample_rate(spec, model_config.sample_rate)
        })?
        .build(transcriber);

    debug!("built audio pipeline");

    let shutdown = Shutdown::new();

    let (adapter_handle, audio_cb) =
        mpsc_adapter.spawn(audio_pipeline, shutdown.clone())?;

    let mut stream = device.stream(audio_cb)?;

    debug!("setup ctrl-c handler");
    let shutdown_ctrlc = shutdown.clone();

    ctrlc::set_handler(move || shutdown_ctrlc.request())?;

    debug!("starting audio stream");
    stream.play()?;

    shutdown.wait();

    drop(stream);
    debug!("joining adapter thread");
    adapter_handle.join()?;

    debug!("done");

    Ok(())
}
