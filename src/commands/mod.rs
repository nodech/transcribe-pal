mod common;

#[cfg(all(unix, feature = "daemon"))]
pub(crate) mod daemon;

pub(crate) mod enumerate;
pub(crate) mod model;
pub(crate) mod transcribe;
