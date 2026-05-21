use std::error::Error as StdError;
use std::num::NonZeroUsize;
use std::sync::mpsc;
use std::sync::mpsc::TrySendError;
use std::thread;
use std::thread::JoinHandle;
use thiserror::Error;

use super::AudioCallbackConsumer;
use super::AudioConsumer;

#[derive(Debug, Error)]
pub enum MPSCAudioCallbackError<E>
where
    E: StdError + Send + Sync + 'static,
{
    #[error("Thread failed to join")]
    JoinFailed,

    #[error("Audio consumer failed: {0}")]
    AudioConsumerError(#[from] E),

    #[error("Sending data failed: {0}")]
    AudioSendError(E),
}

pub struct MPSCAudioAdapter {
    items: usize,
}

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

    pub fn spawn<W: AudioConsumer + Send + 'static>(
        &self,
        mut consumer: W,
    ) -> Result<
        (
            MPSCAudioAdapterHandle<W>,
            impl AudioCallbackConsumer + 'static,
        ),
        MPSCAudioCallbackError<W::Error>,
    > {
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(self.items);
        let handle = thread::spawn(move || {
            while let Ok(samples) = rx.recv() {
                consumer.push_chunk(&samples)?;
            }

            consumer.finish()?;
            Ok(consumer)
        });

        Ok((
            MPSCAudioAdapterHandle { inner: handle },
            MPSCAudioCallback { tx },
        ))
    }
}

impl<W: AudioConsumer + Send + 'static> MPSCAudioAdapterHandle<W> {
    pub fn join(self) -> Result<W, MPSCAudioCallbackError<W::Error>> {
        let Ok(consumer) = self.inner.join() else {
            return Err(MPSCAudioCallbackError::JoinFailed);
        };

        consumer.map_err(Into::into)
    }
}

impl AudioCallbackConsumer for MPSCAudioCallback {
    type Error = MPSCAudioCallbackError<TrySendError<Vec<f32>>>;

    fn try_push_chunk(&mut self, samples: &[f32]) -> Result<(), Self::Error> {
        self.tx
            .try_send(samples.to_vec())
            .map_err(MPSCAudioCallbackError::AudioSendError)?;

        Ok(())
    }
}
