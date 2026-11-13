use std::{
    fmt::Debug,
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
pub struct SimpleShutdown {
    requested: Arc<AtomicBool>,
    poll_timeout: Duration,
}

impl Default for SimpleShutdown {
    fn default() -> Self {
        Self::with_poll_time(DEFAULT_POLL_TIME)
    }
}

impl SimpleShutdown {
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

impl AtomicShutdown for SimpleShutdown {
    fn is_shutdown(&self) -> bool {
        self.is_requested()
    }

    fn request_shutdown(&self) {
        self.request();
    }
}

pub trait AtomicShutdown: Send + Sync {
    fn is_shutdown(&self) -> bool;
    fn request_shutdown(&self);
}

pub struct ShutdownController<'a> {
    poll_interval: Duration,
    registered: Vec<Arc<dyn AtomicShutdown + 'a>>,
}

impl Default for ShutdownController<'_> {
    fn default() -> Self {
        Self::new(DEFAULT_POLL_TIME)
    }
}

impl<'a> ShutdownController<'a> {
    pub fn new(interval: Duration) -> Self {
        ShutdownController { poll_interval: interval, registered: vec![] }
    }

    pub fn register(&mut self, s: Arc<dyn AtomicShutdown + 'a>) {
        self.registered.push(s);
    }

    pub fn wait(&self) {
        if self.registered.is_empty() {
            return;
        }

        while !self.registered.iter().any(|r| r.is_shutdown()) {
            sleep(self.poll_interval);
        }

        self.registered.iter().for_each(|r| r.request_shutdown());

        while !self.registered.iter().all(|r| r.is_shutdown()) {
            sleep(self.poll_interval);
        }
    }
}
