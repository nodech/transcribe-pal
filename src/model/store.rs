use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    PROJECT_NAME,
    lockfile::{LockFile, LockFileError, acquire_lock_file},
    model::{FileSize, manifest::ModelManifest},
};

#[derive(Debug, thiserror::Error)]
#[error("Could not determine the store directory")]
pub struct DetermineDirectoryError;

#[derive(Debug, Clone)]
pub struct StoreDirectoryPath(PathBuf);

#[derive(Debug, Clone, Copy)]
pub struct FileEntry {
    pub size: FileSize,
}

#[derive(Debug)]
pub struct DirectoryContents {
    files: BTreeMap<PathBuf, FileEntry>,
}

pub trait Backend {
    type Error: std::error::Error + Send + Sync + 'static;

    fn init_directory(&mut self, path: &Path) -> Result<(), Self::Error>;
    fn list_dir(
        &mut self,
        path: &Path,
    ) -> Result<BTreeMap<PathBuf, FileEntry>, Self::Error>;
}

pub struct FSBackend;

pub struct Store<T: Backend> {
    backend: T,
    root_dir: StoreDirectoryPath,
}

pub struct ModelStore<'s, 'm, T: Backend> {
    store: &'s mut Store<T>,
    manifest: &'m ModelManifest,
    model_dir: PathBuf,
}

impl StoreDirectoryPath {
    pub fn try_default() -> Result<Self, DetermineDirectoryError> {
        dirs::data_local_dir()
            .map(|mut d| {
                d.push(PROJECT_NAME);
                Self(d)
            })
            .ok_or(DetermineDirectoryError)
    }

    pub fn from_opt_path(
        dir: Option<PathBuf>,
    ) -> Result<Self, DetermineDirectoryError> {
        match dir {
            Some(pb) => Ok(Self(pb)),
            None => Self::try_default(),
        }
    }

    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }
}

impl DirectoryContents {
    pub fn get(&self, path: &Path) -> Option<FileEntry> {
        self.files.get(path).copied()
    }
}

impl FSBackend {
    pub fn new() -> Self {
        Self {}
    }
}

impl Backend for FSBackend {
    type Error = io::Error;

    fn init_directory(&mut self, path: &Path) -> io::Result<()> {
        fs::DirBuilder::new().recursive(true).create(path)
    }

    fn list_dir(
        &mut self,
        path: &Path,
    ) -> Result<BTreeMap<PathBuf, FileEntry>, Self::Error> {
        let dir_files = fs::read_dir(path)?;
        let mut files: BTreeMap<PathBuf, FileEntry> = BTreeMap::new();

        for d in dir_files {
            let entry = d?;
            let metadata = entry.metadata()?;

            if !metadata.is_file() {
                continue;
            }

            let file_size = metadata.len();
            let pb = PathBuf::from(entry.file_name());

            files.insert(pb, FileEntry { size: file_size });
        }

        Ok(files)
    }
}

impl<T: Backend> Store<T> {
    pub fn new(root: StoreDirectoryPath, backend: T) -> Self {
        Self {
            backend,
            root_dir: root,
        }
    }

    pub fn path(&self) -> &Path {
        self.root_dir.as_path()
    }

    pub fn acquire_lock(&self) -> Result<LockFile, LockFileError> {
        acquire_lock_file(self.root_dir.as_path())
    }

    pub fn ensure_dir(&mut self) -> Result<(), T::Error> {
        self.backend.init_directory(self.root_dir.as_path())
    }
}

impl<'s, 'm, T: Backend> ModelStore<'s, 'm, T> {
    pub fn from_store(
        store: &'s mut Store<T>,
        manifest: &'m ModelManifest,
    ) -> Self {
        let model_dir = store.root_dir.as_path().join(manifest.model_path());

        Self {
            model_dir,
            store,
            manifest,
        }
    }

    pub fn model_path(&self) -> &Path {
        self.model_dir.as_path()
    }

    pub fn manifest(&self) -> &ModelManifest {
        self.manifest
    }

    pub fn ensure_dir(&mut self) -> Result<(), T::Error> {
        self.store.backend.init_directory(self.model_dir.as_path())
    }

    pub fn list_dir(&mut self) -> Result<DirectoryContents, T::Error> {
        let files = self.store.backend.list_dir(&self.model_dir)?;

        Ok(DirectoryContents { files })
    }
}
