use crate::transcribe::{
    MAX_SPEECH_END_DELAY_MS, MIN_SPEECH_END_DELAY_MS, ModelKind,
};
use anyhow::anyhow;
use clap::{Args, ValueEnum};
use tracing::debug;

use crate::{
    model::{self, FSBackend, ModelStore, Store, StoreDirectoryPath},
    output::{IoWriter, MultiWriter},
    transcribe::ModelConfig,
};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum CommandModelKind {
    /// This does not support direct streaming, but can "stream" with VAD.
    Parakeet,
}

impl From<CommandModelKind> for ModelKind {
    fn from(val: CommandModelKind) -> Self {
        match val {
            CommandModelKind::Parakeet => ModelKind::Parakeet,
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct TranscriberCommandArgs {
    #[arg(long, value_enum, default_value_t = CommandModelKind::Parakeet)]
    pub(super) model: CommandModelKind,

    /// Microphone threshold (0.0 - 1.0)
    #[arg(long)]
    pub(super) mic_threshold: Option<f32>,

    #[arg(
        long,
        help = format!("Speech end delay in milliseconds ({MIN_SPEECH_END_DELAY_MS} - {MAX_SPEECH_END_DELAY_MS})"),
        value_parser = clap::value_parser!(u64).range(
            MIN_SPEECH_END_DELAY_MS..=MAX_SPEECH_END_DELAY_MS)
    )]
    pub(super) speech_end_delay: Option<u64>,
}

#[derive(Debug, Args)]
pub(crate) struct AudioInputCommandArgs {
    /// Audio host on the system
    #[arg(long)]
    pub(super) host: Option<String>,
    /// Audio device on the host
    #[arg(long)]
    pub(super) device: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct MultiWriterOptions {
    #[cfg(feature = "wayland")]
    pub(super) wtype: bool,
    pub(super) stdout: bool,
}

pub(super) fn multi_writer(
    options: impl Into<MultiWriterOptions>,
) -> anyhow::Result<MultiWriter> {
    let MultiWriterOptions {
        #[cfg(feature = "wayland")]
        wtype,
        stdout,
    } = options.into();

    let mut multi = MultiWriter::new();

    #[cfg(feature = "wayland")]
    if wtype {
        use crate::output::WTypeWriter;

        debug!("enabled wtype writer");
        multi.push_writer(WTypeWriter::new());
    }

    if stdout {
        debug!("enabled stdout writer");
        multi.push_writer(IoWriter::stdout());
    }

    if multi.is_empty() {
        return Err(anyhow::anyhow!("no transcript output is configured"));
    }

    Ok(multi)
}

pub(super) fn init_model_store(
    store_dir: StoreDirectoryPath,
    model_kind: ModelKind,
) -> anyhow::Result<ModelConfig> {
    let model_manifests = model::load_manifests()?;
    let manifest = model_manifests.get(model_kind.as_str()).ok_or(anyhow!(
        "Model {} is not supported (No manifest).",
        model_kind.as_str()
    ))?;

    let fs_backend = FSBackend::new();
    let mut store = Store::new(store_dir, fs_backend);
    let model_store = ModelStore::from_store(&mut store, manifest);

    Ok(ModelConfig::default()
        .with_path_opt(Some(model_store.model_path()))
        .with_kind(model_kind))
}
