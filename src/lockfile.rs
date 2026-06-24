use std::path::PathBuf;
use thiserror::Error;
use tracing::error;

#[derive(Debug)]
pub struct LockFile(std::fs::File);

impl Drop for LockFile {
    fn drop(&mut self) {
        if let Err(e) = self.0.unlock() {
            error!("Error unlocking file: {}", e);
        }
    }
}

#[derive(Debug, Error)]
pub enum LockFileError {
    #[error("LockFile IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("LockFile failed to acquire lock.")]
    TryLockError(#[from] std::fs::TryLockError),
}

pub fn acquire_lock_file(dir: PathBuf) -> Result<LockFile, LockFileError> {
    let filename = dir.join("LOCK");

    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(filename)
        .map_err(LockFileError::Io)?;

    file.try_lock()?;
    let lock = LockFile(file);

    Ok(lock)
}
