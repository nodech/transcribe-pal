use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use tracing::{debug, instrument};

use crate::{
    audio::{
        device::AudioDeviceBuilder,
        device_cb::MPSCAudioAdapter,
        pipeline::{PipelineBuilder, RemixerAvg, ResampleProcessor},
    },
    commands::common::{
        AudioInputCommandArgs, TranscriberCommandArgs, init_model_store,
    },
    transcribe,
};

use crate::commands::common::{MultiWriterOptions, multi_writer};

use crate::model::StoreDirectoryPath;
use crate::shutdown::Shutdown;
use crate::transcribe::ModelKind;

#[derive(Debug, Args)]
struct MultiWriterCommandArgs {
    /// Do not print to stdout
    #[arg(long)]
    no_stdout: bool,

    #[cfg(feature = "wayland")]
    /// Print text using wtype
    #[arg(long)]
    wtype: bool,
}

impl From<MultiWriterCommandArgs> for MultiWriterOptions {
    fn from(args: MultiWriterCommandArgs) -> Self {
        MultiWriterOptions {
            stdout: !args.no_stdout,

            #[cfg(feature = "wayland")]
            wtype: args.wtype,
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct TranscribeCommandArgs {
    #[command(flatten)]
    audio_input_args: AudioInputCommandArgs,

    #[command(flatten)]
    multi_writer_args: MultiWriterCommandArgs,

    /// Path to the store
    #[arg(long)]
    store_dir: Option<PathBuf>,

    #[command(flatten)]
    transcriber_args: TranscriberCommandArgs,
}

#[instrument(level = "info", name = "transcribe", skip_all)]
pub(crate) fn run(cmd_args: TranscribeCommandArgs) -> anyhow::Result<()> {
    debug!(command_args = ?cmd_args, "transcribe");

    let TranscribeCommandArgs {
        audio_input_args,
        multi_writer_args,
        store_dir,
        transcriber_args,
    } = cmd_args;

    let model_kind = ModelKind::from(transcriber_args.model);

    let store_dir = StoreDirectoryPath::from_opt_path(store_dir)?;
    let model = init_model_store(store_dir, model_kind)?;
    let model_config = model.audio_conig();

    let multi = multi_writer(multi_writer_args)?;

    debug!(model = ?model, model_audio_config = ?model_config,
        "model configs");

    let transcriber = transcribe::AudioTranscriberBuilder::default()
        .try_with_mic_threshold_opt(transcriber_args.mic_threshold)?
        .try_with_speech_end_delay_opt(
            transcriber_args.speech_end_delay.map(Duration::from_millis),
        )?
        .with_model(model)
        .build(multi)?;

    debug!("built transcriber");

    let mut device = AudioDeviceBuilder::new()
        .with_host(audio_input_args.host)
        .with_device(audio_input_args.device)
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

    let mpsc_adapter = MPSCAudioAdapter::new(NonZeroUsize::try_from(100)?);

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
