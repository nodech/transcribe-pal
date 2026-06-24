use std::{fs, marker::PhantomData, path::Path, str::FromStr};

use sha1::Digest;

#[derive(Debug, thiserror::Error)]
#[error("Could not parse hash")]
pub struct IncorrectHash;

pub trait HashKind {
    const HEX_SIZE: usize;
    type Hasher: Digest;
}

#[derive(Debug, PartialEq, Eq)]
pub struct Sha1;

impl HashKind for Sha1 {
    type Hasher = sha1::Sha1;
    const HEX_SIZE: usize = 40;
}

#[derive(Debug, PartialEq, Eq)]
pub struct Sha256;

impl HashKind for Sha256 {
    type Hasher = sha2::Sha256;
    const HEX_SIZE: usize = 64;
}

#[derive(Debug, PartialEq, Eq)]
pub struct Hash<T: HashKind> {
    inner: String,
    _size: PhantomData<T>,
}

impl<T: HashKind> Hash<T> {
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl<T: HashKind> FromStr for Hash<T> {
    type Err = IncorrectHash;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != T::HEX_SIZE || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(IncorrectHash);
        }

        Ok(Self {
            inner: s.to_owned(),
            _size: PhantomData,
        })
    }
}

pub fn hash_file<H: HashKind>(file: &Path) -> std::io::Result<Hash<H>> {
    fs::read_to_string(file).map(|c| {
        let mut hasher = H::Hasher::new();
        hasher.update(c);

        Hash::<H> {
            inner: hex_encode(hasher.finalize()),
            _size: PhantomData,
        }
    })
}

const HEX_CHARS_LOWER: &[u8; 16] = b"0123456789abcdef";

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);

    for &byte in bytes {
        out.push(HEX_CHARS_LOWER[(byte >> 4) as usize] as char);
        out.push(HEX_CHARS_LOWER[(byte & 0xf) as usize] as char);
    }

    out
}
