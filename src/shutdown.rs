use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::sleep,
    time::Duration,
};

const ATOMIC_ORDERING: Ordering = Ordering::SeqCst;
const DEFAULT_POLL_TIME: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub struct Shutdown {
    requested: Arc<AtomicBool>,
    poll_timeout: Duration,
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::with_poll_time(DEFAULT_POLL_TIME)
    }
}

impl Shutdown {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_poll_time(time: Duration) -> Self {
        Self {
            requested: Arc::new(AtomicBool::new(false)),
            poll_timeout: time,
        }
    }

    pub fn request(&self) {
        self.requested.store(true, ATOMIC_ORDERING);
    }

    pub fn is_requested(&self) -> bool {
        self.requested.load(ATOMIC_ORDERING)
    }

    pub fn wait(&self) {
        while !self.is_requested() {
            sleep(self.poll_timeout);
        }
    }
}
