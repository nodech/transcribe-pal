use std::{
    collections::BTreeSet,
    fs,
    os::unix::{
        self,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{
            AtomicBool,
            Ordering::{self, SeqCst},
        },
    },
    thread::{self, JoinHandle},
};

use anyhow::Result;
use thiserror::Error;
use tracing::{debug, instrument};

use crate::shutdown::{AtomicShutdown, ShutdownController};

const ATOMIC_ORDERING: Ordering = SeqCst;

struct Connection {}

#[derive(Debug, Error)]
pub(super) enum APIServerError {
    #[error("Failed to bind to sock file: {0}")]
    BindError(std::io::Error),

    #[error("Failed to remove sock file: {0}")]
    RemoveError(std::io::Error),
}

pub(super) struct APIServerHandle {
    thread_handle: JoinHandle<Result<(), APIServerError>>,
    socket_path: Arc<Path>,
    shutdown_flag: Arc<AtomicBool>,
}

impl APIServerHandle {
    pub(super) fn join(self) -> thread::Result<Result<(), APIServerError>> {
        self.thread_handle.join()
    }
}

impl AtomicShutdown for APIServerHandle {
    fn is_shutdown(&self) -> bool {
        self.thread_handle.is_finished()
    }

    fn request_shutdown(&self) {
        self.shutdown_flag.store(true, ATOMIC_ORDERING);

        debug!("shutting down server.");
        let res = UnixStream::connect(&self.socket_path);
        drop(res)
    }
}

struct APIServer<'a> {
    socket_path: Arc<Path>,
    shutdown_flag: Arc<AtomicBool>,
    connections: BTreeSet<Connection>,
    shutdown_ctrl: ShutdownController<'a>,
}

impl<'a> APIServer<'a> {
    #[instrument(level = "debug", name = "api-server.spawn", skip_all)]
    fn run(&mut self) -> Result<(), APIServerError> {
        let bind = UnixListener::bind(&self.socket_path)
            .map_err(APIServerError::BindError)?;

        debug!("Accepting connections at: {:?}", &self.socket_path);
        while !self.shutdown_flag.load(ATOMIC_ORDERING) {
            let Ok((stream, _)) = bind.accept() else {
                debug!("Failed to accept connection");
                continue;
            };

            if self.shutdown_flag.load(ATOMIC_ORDERING) {
                break;
            }

            debug!("Accepted connection");
            // TODO: spawn connection thread and register shutdowns.
        }

        debug!("closing server...");
        // Wait for all to shutdown.
        self.shutdown_ctrl.request_shutdown();
        self.shutdown_ctrl.wait();

        drop(bind);
        fs::remove_file(&self.socket_path)
            .map_err(APIServerError::RemoveError)?;

        Ok(())
    }
}

#[instrument(level = "debug", name = "api-server:start", skip_all)]
pub fn start(sock_path: PathBuf) -> APIServerHandle {
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let socket_path: Arc<Path> = sock_path.into();

    let mut server = APIServer {
        socket_path: socket_path.clone(),
        connections: BTreeSet::new(),
        shutdown_flag: shutdown_flag.clone(),
        shutdown_ctrl: ShutdownController::default(),
    };

    let handle = thread::spawn(move || {
        debug!("Starting server...");

        let res = server.run();
        debug!("Server stopped.");
        res
    });

    APIServerHandle {
        thread_handle: handle,
        socket_path: socket_path.clone(),
        shutdown_flag: shutdown_flag.clone(),
    }
}
