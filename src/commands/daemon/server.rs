#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use std::collections::BTreeMap;

use std::sync::atomic::{self, AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex};

use std::thread::{self, JoinHandle};

use anyhow::Result;
use thiserror::Error;
use tracing::{debug, instrument};

use crate::shutdown::AtomicShutdown;

const ATOMIC_ORDERING: atomic::Ordering = SeqCst;
const MAX_LINE_LEN: usize = 100;

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

struct APIServer {
    socket_path: Arc<Path>,
    shutdown_flag: Arc<AtomicBool>,
    connection_id: ConnectionId,
    connections: BTreeMap<ConnectionId, ConnectionHandle>,
}

impl APIServer {
    #[instrument(level = "debug", name = "api-server.run", skip_all)]
    fn run(&mut self) -> Result<(), APIServerError> {
        let bind = UnixListener::bind(&self.socket_path)
            .map_err(APIServerError::BindError)?;

        debug!("Accepting connections at: {:?}", &self.socket_path);
        while !self.shutdown_flag.load(ATOMIC_ORDERING) {
            let Ok((stream, _)) = bind.accept() else {
                debug!("Failed to accept connection");
                continue;
            };

            self.connection_id = self.connection_id.wrapping_add(1);

            if self.shutdown_flag.load(ATOMIC_ORDERING) {
                break;
            }

            match connection_init(stream, self.connection_id) {
                Ok(handle) => {
                    debug!("Accepted connection");
                    self.connections.insert(self.connection_id, handle);
                }
                Err(e) => {
                    eprintln!("Failed to initialize connection: {:?}", e);
                }
            }
        }

        debug!("closing server...");

        self.request_shutdown();
        self.wait_for_shutdown();

        drop(bind);
        fs::remove_file(&self.socket_path)
            .map_err(APIServerError::RemoveError)?;

        Ok(())
    }

    fn request_shutdown(&mut self) {
        for connection in self.connections.values() {
            connection.request_shutdown();
        }
    }

    fn wait_for_shutdown(&mut self) {
        let connections = std::mem::take(&mut self.connections);

        for (_, connection) in connections.into_iter() {
            _ = connection.join()
        }
    }
}

#[instrument(level = "debug", name = "api-server:start", skip_all)]
pub fn start(sock_path: PathBuf) -> APIServerHandle {
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let socket_path: Arc<Path> = sock_path.into();

    let mut server = APIServer {
        socket_path: socket_path.clone(),
        connection_id: 0,
        connections: BTreeMap::new(),
        shutdown_flag: shutdown_flag.clone(),
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

#[derive(Debug, Error)]
enum ConnectionError {
    #[error("Stream clone failed: {0}")]
    StreamClone(std::io::Error),

    #[error("Write to stream failed: {0}")]
    StreamWrite(std::io::Error),

    #[error("Writing mutex paniced")]
    PoisonedMutex,

    #[error("Shutdown failed")]
    Shutdown(std::io::Error),
}

type ConnectionId = u32;

struct ConnectionHandle {
    id: ConnectionId,
    stream_shutdown: UnixStream,
    stream_write: Arc<Mutex<UnixStream>>,
    handle: JoinHandle<Result<(), ConnectionError>>,
}

impl ConnectionHandle {
    fn join(self) -> thread::Result<Result<(), ConnectionError>> {
        self.handle.join()
    }

    pub(super) fn write_all(
        &mut self,
        message: &[u8],
    ) -> Result<(), ConnectionError> {
        let mut stream = self
            .stream_write
            .lock()
            .map_err(|_| ConnectionError::PoisonedMutex)?;

        stream.write_all(message).map_err(ConnectionError::StreamWrite)?;

        Ok(())
    }
}

impl AtomicShutdown for ConnectionHandle {
    fn is_shutdown(&self) -> bool {
        self.handle.is_finished()
    }

    fn request_shutdown(&self) {
        self.stream_shutdown
            .shutdown(std::net::Shutdown::Both)
            .expect("Stream shutdown failed");
    }
}

struct Connection {
    id: ConnectionId,
    max_line_len: usize,
    stream_read: BufReader<UnixStream>,
    stream_write: Arc<Mutex<UnixStream>>,
}

impl Connection {
    #[instrument(level = "debug", name = "api-server:run", skip_all)]
    fn run(&mut self) -> Result<(), ConnectionError> {
        debug!("Connection {} was initialized", self.id);

        let max_read: usize = self.max_line_len + 1;
        let mut take_stream = (&mut self.stream_read).take(max_read as u64);
        let mut line_buffer = String::with_capacity(max_read);

        while let Ok(r) = take_stream.read_line(&mut line_buffer)
            && r > 0
        {
            if !line_buffer.ends_with('\n') {
                debug!("Line too long or connection ended mid-line");
                break;
            }

            // TODO: Process the command

            take_stream = (&mut self.stream_read).take(max_read as u64);
            line_buffer.clear();
        }

        self.stream_read
            .get_ref()
            .shutdown(std::net::Shutdown::Both)
            .map_err(ConnectionError::Shutdown)?;

        debug!("Connection {} stopped", self.id);

        Ok(())
    }
}

#[instrument(level = "debug", name = "connection::init", skip_all)]
fn connection_init(
    stream: UnixStream,
    id: ConnectionId,
) -> Result<ConnectionHandle, ConnectionError> {
    let stream_shutdown =
        stream.try_clone().map_err(ConnectionError::StreamClone)?;

    let stream_write = Arc::new(Mutex::new(
        stream.try_clone().map_err(ConnectionError::StreamClone)?,
    ));

    let stream_read = BufReader::new(stream);

    let mut connection = Connection {
        max_line_len: MAX_LINE_LEN,
        stream_read,
        stream_write: stream_write.clone(),
        id,
    };

    let handle = thread::spawn(move || connection.run());

    Ok(ConnectionHandle { stream_shutdown, stream_write, handle, id })
}
