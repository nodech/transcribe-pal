use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Read, Write},
    ops::RangeFrom,
    path::{Path, PathBuf},
};
use thiserror::Error;
use tracing::{debug, instrument, trace};
use ureq::{
    Body, BodyReader,
    http::{Response, StatusCode},
};

use crate::model::{
    FileSize,
    hash::{Hash, Sha256, hash_file},
    manifest::ModelManifest,
    store::{Backend, ModelStore},
};

const DOWNLOAD_BUF_SIZE: usize = 64 * 1024;

#[derive(Debug)]
pub struct DownloadFileDetails {
    file_path: PathBuf,
    hash: Hash<Sha256>,
    pub downloaded_size: FileSize,
    pub expected_size: FileSize,
}

impl Eq for DownloadFileDetails {}
impl PartialEq for DownloadFileDetails {
    fn eq(&self, other: &Self) -> bool {
        self.file_path == other.file_path
    }
}

impl Ord for DownloadFileDetails {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.file_path.cmp(&other.file_path)
    }
}

impl PartialOrd for DownloadFileDetails {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl DownloadFileDetails {
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn is_done(&self) -> bool {
        self.downloaded_size == self.expected_size
    }
}

pub enum DownloadStage {
    Fetch(DownloadFileDetails),
    Resume(DownloadFileDetails),
    Progress {
        file_details: DownloadFileDetails,
        file: File,
        reader: BodyReader<'static>,
    },
    VerifyQueued(DownloadFileDetails),
    Verify(DownloadFileDetails),
    Done(DownloadFileDetails),
}

impl Eq for DownloadStage {}

impl PartialEq for DownloadStage {
    fn eq(&self, other: &Self) -> bool {
        self.file() == other.file()
    }
}

impl Ord for DownloadStage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.file().cmp(other.file())
    }
}

impl PartialOrd for DownloadStage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("Failed to open file: {0}")]
    IO(#[from] io::Error),

    #[error("Could not fetch url: {0}")]
    FetchUrl(#[from] ureq::Error),

    #[error("Unexpected status code: {0}")]
    FetchStatusCode(StatusCode),

    #[error("Incorrect verification state: {0} != {1}")]
    IncorrectVerificationState(FileSize, FileSize),

    #[error("Incorrect hash {hash} for the file {file}, expected: {expected}")]
    IncorrectHash {
        file: PathBuf,
        hash: Hash<Sha256>,
        expected: Hash<Sha256>,
    },
}

pub struct Download<'m> {
    manifest: &'m ModelManifest,
    files: BTreeSet<DownloadStage>,
    model_path: PathBuf,
    downloaded_size: FileSize,
    total_size: FileSize,
}

pub struct DownloadFile<'d, 'm> {
    download: &'d mut Download<'m>,
    remote_url: url::Url,
    stage: Option<DownloadStage>,
}

#[derive(Debug)]
pub enum DownloadProgress {
    Fetch,
    Resume {
        downloaded: FileSize,
        downloaded_total: FileSize,
    },
    Progress {
        downloaded: FileSize,
        downloaded_total: FileSize,
    },
    Verify,
    Finalize,
    Done,
}

#[derive(Debug)]
enum DownloadOpenOptions {
    Write,
    Append,
}

#[derive(Debug)]
pub struct DownloadRequest {
    downloaded_size: FileSize,
    total_size: FileSize,
    files: BTreeSet<DownloadStage>,
}

impl DownloadRequest {
    pub fn new<T: Backend>(
        store: &mut ModelStore<'_, '_, T>,
    ) -> Result<Self, T::Error> {
        let mut pending = BTreeSet::new();
        let files = store.list_dir()?;
        let manifest = store.manifest();
        let mut downloaded_size: FileSize = 0;
        let mut total_size: FileSize = 0;

        for file in manifest.download_files.values() {
            let path = file.path.as_path();
            let file_path = file.path.to_path_buf();

            let mut file_details = DownloadFileDetails {
                file_path,
                hash: file.hash.clone(),
                downloaded_size: 0,
                expected_size: file.size,
            };

            total_size += file.size;

            let stage = match files.get(path) {
                Some(entry) if entry.size == file.size => {
                    file_details.downloaded_size = entry.size;
                    downloaded_size += entry.size;
                    DownloadStage::VerifyQueued(file_details)
                }
                Some(entry) if entry.size < file.size && entry.size > 0 => {
                    file_details.downloaded_size = entry.size;
                    downloaded_size += entry.size;
                    DownloadStage::Resume(file_details)
                }
                Some(_) | None => DownloadStage::Fetch(file_details),
            };

            pending.insert(stage);
        }

        Ok(DownloadRequest {
            downloaded_size,
            total_size,
            files: pending,
        })
    }
}

impl<'m> Download<'m> {
    pub fn new(
        model_path: PathBuf,
        manifest: &'m ModelManifest,
        request: DownloadRequest,
    ) -> Self {
        let DownloadRequest {
            downloaded_size,
            total_size,
            files,
        } = request;

        Self {
            manifest,
            files,
            model_path,
            downloaded_size,
            total_size,
        }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn downloaded_size(&self) -> FileSize {
        self.downloaded_size
    }

    pub fn total_size(&self) -> FileSize {
        self.total_size
    }

    pub fn next<'d>(&'d mut self) -> Option<DownloadFile<'d, 'm>> {
        self.files.pop_first().map(|ds| DownloadFile::new(self, ds))
    }
}

impl<'d, 'm> DownloadFile<'d, 'm> {
    pub fn new(download: &'d mut Download<'m>, stage: DownloadStage) -> Self {
        let remote_url =
            download.manifest.resolve_url(stage.file().file_path());

        Self {
            download,
            stage: Some(stage),
            remote_url,
        }
    }

    #[instrument(level = "debug", name = "download_file.process", skip_all)]
    pub fn process(&mut self) -> Result<DownloadProgress, DownloadError> {
        let stage = self.stage.take().expect("Stage must exist");

        match stage {
            DownloadStage::Fetch(file_details) => {
                debug!("fetching {:?}", file_details);
                let path = self.store_path(file_details.file_path());
                let file = DownloadOpenOptions::Write.open(&path)?;
                let response = self.request_file_full()?;
                let reader = response.into_body().into_reader();

                self.stage = Some(DownloadStage::Progress {
                    file_details,
                    file,
                    reader,
                });

                Ok(DownloadProgress::Fetch)
            }
            DownloadStage::Resume(mut file_details) => {
                debug!(
                    "continuing download for {:?} from {}",
                    file_details.file_path(),
                    file_details.downloaded_size
                );

                let path = self.store_path(file_details.file_path());
                let response =
                    self.request_file_range(file_details.downloaded_size..)?;

                let status_code = response.status();
                let open_opts = match status_code {
                    StatusCode::PARTIAL_CONTENT => DownloadOpenOptions::Append,
                    StatusCode::OK => {
                        self.download.downloaded_size -=
                            file_details.downloaded_size;
                        file_details.downloaded_size = 0;
                        DownloadOpenOptions::Write
                    }
                    _ => {
                        return Err(DownloadError::FetchStatusCode(
                            status_code,
                        ));
                    }
                };

                let file = open_opts.open(&path)?;
                let reader = response.into_body().into_reader();
                let downloaded = file_details.downloaded_size;

                self.stage = Some(DownloadStage::Progress {
                    file_details,
                    file,
                    reader,
                });

                Ok(DownloadProgress::Resume {
                    downloaded,
                    downloaded_total: self.download.downloaded_size,
                })
            }
            DownloadStage::Progress {
                mut file_details,
                mut file,
                mut reader,
            } => {
                trace!(
                    "progressing file stream \"{}\" {}/{}",
                    file_details.file_path().display(),
                    file_details.downloaded_size,
                    file_details.expected_size
                );

                let mut read_buf = [0u8; DOWNLOAD_BUF_SIZE];

                let n = loop {
                    match reader.read(&mut read_buf) {
                        Ok(n) => break n,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::Interrupted =>
                        {
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    }
                };

                file.write_all(&read_buf[..n])?;

                file_details.downloaded_size += n as FileSize;
                self.download.downloaded_size += n as FileSize;

                let downloaded = file_details.downloaded_size;

                if n == 0 {
                    debug!("download finished");
                    self.stage = Some(DownloadStage::Verify(file_details));
                    Ok(DownloadProgress::Verify)
                } else {
                    self.stage = Some(DownloadStage::Progress {
                        file_details,
                        file,
                        reader,
                    });
                    Ok(DownloadProgress::Progress {
                        downloaded,
                        downloaded_total: self.download.downloaded_size,
                    })
                }
            }
            DownloadStage::VerifyQueued(file_details) => {
                self.stage = Some(DownloadStage::Verify(file_details));
                Ok(DownloadProgress::Verify)
            }
            DownloadStage::Verify(file_details) => {
                if !file_details.is_done() {
                    return Err(DownloadError::IncorrectVerificationState(
                        file_details.downloaded_size,
                        file_details.expected_size,
                    ));
                }

                let path = file_details.file_path();
                let full_path = self.store_path(path);
                debug!("verifying hash for the file: {:?}", path);
                let sum = hash_file::<Sha256>(&full_path)?;

                if sum != file_details.hash {
                    return Err(DownloadError::IncorrectHash {
                        file: path.into(),
                        hash: sum,
                        expected: file_details.hash,
                    });
                }

                self.stage = Some(DownloadStage::Done(file_details));

                Ok(DownloadProgress::Finalize)
            }
            DownloadStage::Done(file_details) => {
                debug!("we are done with: {:?}", file_details.file_path());
                Ok(DownloadProgress::Done)
            }
        }
    }

    pub fn expected_size(&self) -> Option<FileSize> {
        self.stage.as_ref().map(|s| s.file().expected_size)
    }

    pub fn downloaded_size(&self) -> Option<FileSize> {
        self.stage.as_ref().map(|s| s.file().downloaded_size)
    }

    pub fn file_path(&self) -> &Path {
        self.stage
            .as_ref()
            .map(|f| f.file().file_path())
            .expect("Stage must exist")
    }

    pub fn store_path(&self, path: &Path) -> PathBuf {
        self.download.model_path.join(path)
    }

    fn request_file_range(
        &mut self,
        range: RangeFrom<FileSize>,
    ) -> Result<Response<Body>, DownloadError> {
        self.request_file(Some(range))
    }

    fn request_file_full(&mut self) -> Result<Response<Body>, DownloadError> {
        self.request_file(None)
    }

    fn request_file(
        &mut self,
        range: Option<RangeFrom<FileSize>>,
    ) -> Result<Response<Body>, DownloadError> {
        let mut req = ureq::get(self.remote_url.as_str())
            .config()
            .max_redirects(3)
            .build();

        req = req.header("Accept-Encoding", "identity");

        if let Some(RangeFrom { start }) = range {
            req = req.header("Range", format!("bytes={}-", start));
        }

        Ok(req.call()?)
    }
}

impl DownloadStage {
    fn file(&self) -> &DownloadFileDetails {
        match self {
            DownloadStage::Fetch(file_details) => file_details,
            DownloadStage::Resume(file_details) => file_details,
            DownloadStage::Progress { file_details, .. } => file_details,
            DownloadStage::VerifyQueued(file_details) => file_details,
            DownloadStage::Verify(file_details) => file_details,
            DownloadStage::Done(file_details) => file_details,
        }
    }
}

impl std::fmt::Debug for DownloadStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadStage::Fetch(file_details) => {
                f.debug_tuple("Fetch").field(file_details).finish()
            }
            DownloadStage::Resume(file_details) => {
                f.debug_tuple("Resume").field(file_details).finish()
            }
            DownloadStage::Progress {
                file_details, file, ..
            } => f
                .debug_struct("Progress")
                .field("file_details", file_details)
                .field("file", file)
                .finish(),
            DownloadStage::VerifyQueued(file_details) => {
                f.debug_tuple("VerifyQueued").field(file_details).finish()
            }
            DownloadStage::Verify(file_details) => {
                f.debug_tuple("Verify").field(file_details).finish()
            }
            DownloadStage::Done(file_details) => {
                f.debug_tuple("Done").field(file_details).finish()
            }
        }
    }
}

impl DownloadOpenOptions {
    pub fn open(self, path: &Path) -> Result<File, io::Error> {
        match self {
            DownloadOpenOptions::Write => open_truncated_file(path),
            DownloadOpenOptions::Append => open_append_file(path),
        }
    }
}

fn open_truncated_file(path: &Path) -> Result<File, io::Error> {
    fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
}

fn open_append_file(path: &Path) -> Result<File, io::Error> {
    fs::OpenOptions::new().append(true).open(path)
}
