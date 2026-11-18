#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;
use std::{fs, io};

use anyhow::anyhow;
use clap::Args;
use tracing::{debug, instrument};

use crate::lockfile::acquire_lock_file;
use crate::{PROJECT_NAME, audio};

use crate::commands::common::{
    CommandModelKind, MultiWriterOptions, multi_writer,
};

use crate::model::{self, StoreDirectoryPath};
use crate::shutdown::{ShutdownAllController, SimpleShutdown};
use crate::transcribe::{
    self, MAX_SPEECH_END_DELAY_MS, MIN_SPEECH_END_DELAY_MS, ModelConfig,
    ModelKind,
};

mod server;

#[derive(Debug, Args)]
pub(crate) struct DaemonCommandArgs {
    /// Audio host on the system
    #[arg(long)]
    host: Option<String>,

    /// Audio device on the host
    #[arg(long)]
    device: Option<String>,

    #[cfg(feature = "wayland")]
    /// Print text using wtype
    #[arg(long)]
    wtype: bool,

    /// Print to stdout
    #[arg(long)]
    stdout: bool,

    /// Path to the store
    #[arg(long)]
    store_dir: Option<PathBuf>,

    /// Path to the sock dir
    #[arg(long)]
    sock_dir: Option<PathBuf>,

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

#[instrument(level = "info", name = "daemon", skip_all)]
pub(crate) fn run(
    cmd_args: DaemonCommandArgs,
    sshutdown: SimpleShutdown,
) -> anyhow::Result<()> {
    let DaemonCommandArgs {
        host,
        device,
        #[cfg(feature = "wayland")]
        wtype,
        stdout,
        store_dir,
        sock_dir,
        model,
        mic_threshold,
        speech_end_delay,
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
        stdout,
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
        .try_with_speech_end_delay_opt(
            speech_end_delay.map(Duration::from_millis),
        )?
        .with_model(model)
        .build(multi)?;

    debug!("built transcriber");

    let device_config = audio::SessionConfig {
        audio_config: audio::SessionAudioConfig {
            preferred_host: host,
            preferred_device: device,
            buffer_size: Some(2048),
            timeout: Some(Duration::from_secs(1)),

            model_config,
        },
        retry_config: audio::SessionRetryConfig::default(),
    };

    let runtime_dir = sock_dir.unwrap_or_else(|| {
        let mut dir = dirs::runtime_dir().unwrap_or_else(std::env::temp_dir);
        dir.push(PROJECT_NAME);
        dir
    });

    fs::create_dir_all(&runtime_dir)?;

    let sock_file = runtime_dir.join("daemon.sock");
    let lock_file = runtime_dir.join("daemon.lock");

    let _lock = acquire_lock_file(&lock_file)?;

    fs::remove_file(&sock_file).or_else(|e| {
        if e.kind() == io::ErrorKind::NotFound { Ok(()) } else { Err(e) }
    })?;

    let session_handle = audio::Session::start(device_config, transcriber);
    let server_handle = server::APIServer::start(sock_file);

    {
        let mut shutdown = ShutdownAllController::default();
        shutdown.register(&sshutdown);
        shutdown.register(&session_handle);
        shutdown.register(&server_handle);
        shutdown.wait();
    }

    session_handle
        .join()
        .expect("Failed to join thread")
        .map_err(|e| anyhow!("Audio session failed: {:?}", e))?;

    server_handle
        .join()
        .expect("Failed to join thread")
        .map_err(|e| anyhow!("Daemon server failed: {:?}.", e))?;

    Ok(())
}
