use std::error::Error as StdError;
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::sync::mpsc::TrySendError;
use std::thread;
use std::thread::JoinHandle;
use thiserror::Error;
use tracing::{info, instrument};

use crate::shutdown::Shutdown;

use super::AudioCallbackConsumer;
use super::AudioConsumer;

#[derive(Debug, Error)]
pub enum MPSCAudioAdapterError<E>
where
    E: StdError + Send + Sync + 'static,
{
    #[error("Thread failed to join")]
    JoinFailed,

    #[error("IO Error: {0}")]
    IOError(std::io::Error),

    #[error("Audio consumer failed: {0}")]
    AudioConsumerError(#[from] E),
}

pub struct MPSCAudioAdapter {
    items: usize,
}

#[must_use]
pub struct MPSCAudioAdapterHandle<W: AudioConsumer + Send + 'static> {
    inner: JoinHandle<Result<W, W::Error>>,
}

pub struct MPSCAudioCallback {
    tx: mpsc::SyncSender<Vec<f32>>,
}

impl MPSCAudioAdapter {
    pub fn new(item_buffer: NonZeroUsize) -> Self {
        Self {
            items: item_buffer.into(),
        }
    }

    #[instrument(level = "info", name = "mpsc_adapter.spawn", skip_all)]
    pub fn spawn<W: AudioConsumer + Send + 'static>(
        &self,
        mut consumer: W,
        shutdown: Shutdown,
    ) -> Result<
        (
            MPSCAudioAdapterHandle<W>,
            impl AudioCallbackConsumer + 'static,
        ),
        MPSCAudioAdapterError<W::Error>,
    > {
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(self.items);
        info!(items = self.items, "spawning audio chunk consumer thread");

        let handle = thread::Builder::new()
            .name("transcriber_thread".into())
            .spawn(move || {
                while let Ok(samples) = rx.recv() {
                    consumer
                        .push_chunk(&samples)
                        .inspect_err(|_| shutdown.request())?;
                }

                consumer.finish()?;
                Ok(consumer)
            })
            .map_err(MPSCAudioAdapterError::IOError)?;

        Ok((
            MPSCAudioAdapterHandle { inner: handle },
            MPSCAudioCallback { tx },
        ))
    }
}

impl<W: AudioConsumer + Send + 'static> MPSCAudioAdapterHandle<W> {
    pub fn join(self) -> Result<W, MPSCAudioAdapterError<W::Error>> {
        let Ok(consumer) = self.inner.join() else {
            return Err(MPSCAudioAdapterError::JoinFailed);
        };

        consumer.map_err(Into::into)
    }
}

#[derive(Debug, Error)]
pub enum MPSCAudioCallbackError<T> {
    #[error("Failed to send data: {0}")]
    AudioSendError(#[from] TrySendError<T>),
}

impl AudioCallbackConsumer for MPSCAudioCallback {
    type Error = MPSCAudioCallbackError<Vec<f32>>;

    fn try_push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error> {
        self.tx.try_send(samples.to_vec())?;

        Ok(())
    }
}
