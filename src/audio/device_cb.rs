use std::error::Error as StdError;
use std::num::NonZeroUsize;
use std::sync::mpsc;
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
    #[error("Already initialized")]
    AlreadyInitialized,

    #[error("Thread is not running")]
    NotRunning,

    #[error("Thread failed to join")]
    JoinFailed,

    #[error("Audio consumer failed: {0}")]
    AudioConsumerError(#[from] E),
}

pub struct MPSCAudioAdapter<W: AudioConsumer + Send + 'static> {
    handle: Option<JoinHandle<Result<W, W::Error>>>,
    items: usize,
}

pub struct MPSCAudioCallback {
    tx: mpsc::SyncSender<Vec<f32>>,
}

impl<W: AudioConsumer + Send + 'static> MPSCAudioAdapter<W> {
    pub fn new(item_buffer: NonZeroUsize) -> Self {
        Self {
            handle: None,
            items: item_buffer.into(),
        }
    }

    pub fn init(
        &mut self,
        mut consumer: W,
    ) -> Result<
        impl AudioCallbackConsumer + 'static,
        MPSCAudioCallbackError<W::Error>,
    > {
        if self.handle.is_some() {
            return Err(MPSCAudioCallbackError::AlreadyInitialized);
        }

        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(self.items);

        self.handle = Some(thread::spawn(move || {
            while let Ok(samples) = rx.recv() {
                consumer.push_chunk(&samples)?;
            }

            consumer.finish()?;
            Ok(consumer)
        }));

        Ok(MPSCAudioCallback { tx })
    }

    pub fn join(&mut self) -> Result<W, MPSCAudioCallbackError<W::Error>> {
        if self.handle.is_none() {
            return Err(MPSCAudioCallbackError::NotRunning);
        }

        let handle = self.handle.take().unwrap();

        let Ok(consumer) = handle.join() else {
            return Err(MPSCAudioCallbackError::JoinFailed);
        };

        consumer.map_err(Into::into)
    }
}

impl AudioCallbackConsumer for MPSCAudioCallback {
    fn try_push_chunk(&mut self, samples: &[f32]) -> anyhow::Result<()> {
        self.tx.try_send(samples.to_vec())?;
        Ok(())
    }
}
