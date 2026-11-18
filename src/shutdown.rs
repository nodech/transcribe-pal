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
    // It is not guaranteed it will be called once.
    fn request_shutdown(&self);
}

pub struct ShutdownAllController<'a> {
    poll_interval: Duration,
    registered: Vec<&'a dyn AtomicShutdown>,
    requested: Arc<AtomicBool>,
}

impl Default for ShutdownAllController<'_> {
    fn default() -> Self {
        Self::new(DEFAULT_POLL_TIME)
    }
}

impl<'a> ShutdownAllController<'a> {
    pub fn new(interval: Duration) -> Self {
        ShutdownAllController {
            poll_interval: interval,
            registered: vec![],
            requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn register(&mut self, s: &'a dyn AtomicShutdown) {
        self.registered.push(s);
    }

    pub fn wait(&self) {
        if self.registered.is_empty() {
            return;
        }

        while !self.registered.iter().any(|r| r.is_shutdown()) {
            sleep(self.poll_interval);
        }

        self.requested.store(true, ATOMIC_ORDERING);
        self.registered.iter().for_each(|r| r.request_shutdown());

        while !self.registered.iter().all(|r| r.is_shutdown()) {
            sleep(self.poll_interval);
        }
    }
}

impl<'a> AtomicShutdown for ShutdownAllController<'a> {
    fn request_shutdown(&self) {
        if let Some(r) = self.registered.first() {
            r.request_shutdown();
        }
    }

    fn is_shutdown(&self) -> bool {
        self.requested.load(ATOMIC_ORDERING)
    }
}
