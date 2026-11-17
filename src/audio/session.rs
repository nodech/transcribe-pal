use std::{
    fmt::Debug,
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    thread::{self, JoinHandle, spawn},
    time::Duration,
};

use anyhow::Result;
use thiserror::Error;
use tracing::{debug, instrument};

use crate::{
    audio::{
        AudioConsumer, SampleFormat,
        device::{AudioDevice, AudioDeviceBuilder, AudioDeviceBuilderError},
        pipeline::{
            Pipeline, PipelineBuilder, PipelineError, RemixerAvg,
            ResampleProcessor,
        },
    },
    shutdown::AtomicShutdown,
};

use super::DeviceConfig;

pub struct SessionRetryConfig {
    pub retry_delay: Duration,
}

impl Default for SessionRetryConfig {
    fn default() -> Self {
        Self { retry_delay: Duration::from_secs(1) }
    }
}

pub struct SessionAudioConfig {
    pub preferred_host: Option<String>,
    pub preferred_device: Option<String>,
    pub buffer_size: Option<usize>,
    pub timeout: Option<Duration>,

    pub model_config: DeviceConfig,
}

pub struct SessionConfig {
    pub retry_config: SessionRetryConfig,
    pub audio_config: SessionAudioConfig,
}

#[derive(Debug, PartialEq)]
enum Message {
    Shutdown,
    Error,
}

#[derive(Debug, Error)]
pub enum SessionStateError {
    #[error("Incorrect state transition")]
    IncorrectState,
}

enum SessionState<T: AudioConsumer + Send> {
    NotInit { sink: T },
    Initializing,
    Initialized,
}

impl<T: AudioConsumer + Send> Debug for SessionState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInit { .. } => f.write_str("NotInit"),
            Self::Initializing => f.write_str("Initializing"),
            Self::Initialized { .. } => f.write_str("Initialized"),
        }
    }
}

impl<T: AudioConsumer + Send> SessionState<T> {
    fn start_init(&mut self) -> Result<T, SessionStateError> {
        match std::mem::replace(self, Self::Initializing) {
            Self::NotInit { sink } => Ok(sink),
            previous => {
                *self = previous;
                Err(SessionStateError::IncorrectState)
            }
        }
    }

    fn initted(&mut self) -> Result<(), SessionStateError> {
        match std::mem::replace(self, Self::Initialized) {
            Self::Initializing => Ok(()),
            previous => {
                *self = previous;
                Err(SessionStateError::IncorrectState)
            }
        }
    }

    fn deinit(&mut self, sink: T) -> Result<(), SessionStateError> {
        match std::mem::replace(self, Self::NotInit { sink }) {
            Self::Initialized => Ok(()),
            previous => {
                *self = previous;
                Err(SessionStateError::IncorrectState)
            }
        }
    }
}

pub struct SessionHandle {
    shutdown_handle: SyncSender<Message>,
    device_handle: JoinHandle<Result<(), RetryStopped>>,
}

impl SessionHandle {
    pub fn join(self) -> thread::Result<Result<(), RetryStopped>> {
        self.device_handle.join()
    }
}

impl AtomicShutdown for SessionHandle {
    fn is_shutdown(&self) -> bool {
        self.device_handle.is_finished()
    }

    fn request_shutdown(&self) {
        if let Err(e) = self.shutdown_handle.send(Message::Shutdown) {
            println!("Failed to shutdown: {e}.");
        }
    }
}

pub struct Session<T: AudioConsumer + Send> {
    config: SessionConfig,

    lifecycle_tx: SyncSender<Message>,
    lifecycle_rx: Receiver<Message>,

    state: SessionState<T>,
}

#[instrument(level = "debug", name = "audio_session:start", skip_all)]
pub(crate) fn start<T>(config: SessionConfig, sink: T) -> SessionHandle
where
    T: AudioConsumer + Send + 'static,
{
    let (tx, rx) = mpsc::sync_channel::<Message>(10);

    let session = Session {
        config,
        lifecycle_tx: tx.clone(),
        lifecycle_rx: rx,
        state: SessionState::NotInit { sink },
    };

    let handle = spawn(move || {
        debug!("Starting audio session...");
        let mut audio_session = session;

        let res = retry_loop(
            &mut audio_session,
            |audio_session| audio_session.run().into(),
            |audio_session| audio_session.wait(),
        );

        debug!("Audio session stopped.");
        res
    });

    SessionHandle { shutdown_handle: tx, device_handle: handle }
}

impl<T: AudioConsumer + Send + 'static> Session<T> {
    fn wait(&mut self) -> Result<(), RetryStopped> {
        match self
            .lifecycle_rx
            .recv_timeout(self.config.retry_config.retry_delay)
        {
            Err(RecvTimeoutError::Timeout) => Ok(()),
            Err(RecvTimeoutError::Disconnected) => {
                Err(RetryStopped::Disconnected)
            }
            Ok(Message::Shutdown) => Err(RetryStopped::Shutdown),
            Ok(_) => Err(RetryStopped::UnexpectedMessage),
        }
    }

    #[instrument(level = "debug", name = "audio_session.run", skip_all)]
    fn run(&mut self) -> Result<(), AttemptError> {
        debug!("init device");
        let mut device = try_init_device(&self.config.audio_config)?;
        let device_cfg = device.audio_config();

        let (audio_send, audio_recv) = mpsc::sync_channel::<Vec<f32>>(100);

        debug!("init pipeline");
        let pipeline_builder =
            init_pipeline(device_cfg, &self.config.audio_config)?;
        let sink = self.state.start_init()?;
        let mut pipeline = pipeline_builder.build(sink);

        let handle = thread::Builder::new()
            .name("transcriber_thread".into())
            .spawn(move || -> Result<Pipeline<T>, PipelineError> {
                while let Ok(samples) = audio_recv.recv() {
                    pipeline
                        .push_chunk(&samples)
                        .inspect_err(|_| debug!("OKAY TODO ERROR..."))?;
                }

                pipeline.finish()?;
                Ok(pipeline)
            })
            .map_err(|e| {
                debug!("failed to spawn a thread, error: {:?}.", e);
                AttemptError::Unknown
            })?;

        let notifier = self.lifecycle_tx.clone();
        let stream = device.stream_with_error(
            move |samples: &[f32]| -> Result<(), AttemptError> {
                audio_send
                    .try_send(samples.to_vec())
                    .map_err(|_| AttemptError::AudioSendError)?;

                Ok(())
            },
            move |_| {
                _ = notifier.send(Message::Error);
            },
        );

        self.state.initted()?;

        let msg_res = self.lifecycle_rx.recv();

        debug!("Stopping the stream");
        drop(stream);

        debug!("joining the thread");
        let pipeline = handle.join().unwrap()?;

        self.state.deinit(pipeline.sink())?;

        let Ok(msg) = msg_res else {
            debug!("lifecycle channel closed");
            return Ok(());
        };

        match msg {
            Message::Shutdown => {
                debug!("shutting down...")
            }
            Message::Error => {
                debug!("stream error");
                return Err(AttemptError::StreamError);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum InitDeviceError {
    #[error(transparent)]
    DeviceInitError(#[from] AudioDeviceBuilderError),

    #[error(
        "Unsupported format: {device_format:?}, expected: {model_format:?}"
    )]
    UnsupportedFormat {
        model_format: SampleFormat,
        device_format: SampleFormat,
    },
}

fn try_init_device(
    audio_config: &SessionAudioConfig,
) -> Result<AudioDevice, InitDeviceError> {
    let device = AudioDeviceBuilder::new()
        .with_host(audio_config.preferred_host.clone())
        .with_device(audio_config.preferred_device.clone())
        .with_config(audio_config.model_config)
        .with_buffer_size(audio_config.buffer_size)
        .with_timeout(audio_config.timeout)
        .build()?;

    let device_format = device.audio_config().format;
    let model_format = audio_config.model_config.format;

    if device_format != model_format {
        return Err(InitDeviceError::UnsupportedFormat {
            model_format,
            device_format,
        });
    }

    Ok(device)
}

fn init_pipeline(
    device_config: DeviceConfig,
    audio_config: &SessionAudioConfig,
) -> Result<PipelineBuilder, PipelineError> {
    PipelineBuilder::new(device_config)
        .with_stage(|spec| {
            RemixerAvg::to_channels(spec, audio_config.model_config.channels)
        })?
        .with_stage(|spec| {
            ResampleProcessor::to_sample_rate(
                spec,
                audio_config.model_config.sample_rate,
            )
        })
}

#[derive(Debug)]
pub enum RetryStopped {
    Shutdown,
    Disconnected,
    AttemptError,
    UnexpectedMessage,
}

#[derive(Debug, Error)]
pub enum AttemptError {
    #[error(transparent)]
    Init(#[from] InitDeviceError),

    #[error(transparent)]
    Pipeline(#[from] PipelineError),

    #[error("Audio send error")]
    AudioSendError,

    #[error("Stream error")]
    StreamError,

    #[error(transparent)]
    State(#[from] SessionStateError),

    #[error("Unknown error")]
    Unknown,
}

#[derive(Debug)]
pub enum AttemptResult {
    ExitOk,
    ExitError(AttemptError),
    Retry(AttemptError),
}

impl From<Result<(), AttemptError>> for AttemptResult {
    fn from(result: Result<(), AttemptError>) -> Self {
        use AttemptError::*;

        match result {
            Ok(()) => Self::ExitOk,

            Err(
                e @ (Init(_) | Pipeline(_) | AudioSendError | StreamError),
            ) => Self::Retry(e),

            Err(e @ (State(_) | Unknown)) => Self::ExitError(e),
        }
    }
}

#[instrument(level = "debug", name = "retry_loop", skip_all)]
fn retry_loop<S>(
    state: &mut S,
    mut attempt: impl FnMut(&mut S) -> AttemptResult,
    mut wait: impl FnMut(&mut S) -> Result<(), RetryStopped>,
) -> Result<(), RetryStopped> {
    loop {
        let attempt_result = attempt(state);

        match attempt_result {
            AttemptResult::ExitOk => {
                return Ok(());
            }
            AttemptResult::ExitError(err) => {
                debug!("Attempt failed with {:?}, exitting...", err);
                return Err(RetryStopped::AttemptError);
            }
            AttemptResult::Retry(err) => {
                debug!("Attempt failed with {:?}, retrying...", err);
            }
        }

        wait(state)?;
    }
}
