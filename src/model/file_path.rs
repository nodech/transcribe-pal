use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FilePath(String);

impl FilePath {
    pub fn as_path(&self) -> &Path {
        Path::new(self.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.as_str())
    }

    pub fn new(path: String) -> Self {
        Self(path)
    }
}

impl std::fmt::Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::borrow::Borrow<str> for FilePath {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}
