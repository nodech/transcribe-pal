use crate::transcribe::ModelKind;
use clap::ValueEnum;

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

pub(crate) mod enumerate;
pub(crate) mod model;
pub(crate) mod transcribe;
