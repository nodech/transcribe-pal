use std::{marker::PhantomData, str::FromStr};

#[derive(Debug, thiserror::Error)]
#[error("Could not parse hash")]
pub struct IncorrectHash;

pub trait HashSize {
    const SIZE: usize;
}

#[derive(Debug)]
pub struct Sha1;

impl HashSize for Sha1 {
    const SIZE: usize = 40;
}

#[derive(Debug)]
pub struct Sha256;

impl HashSize for Sha256 {
    const SIZE: usize = 64;
}

#[derive(Debug, PartialEq, Eq)]
pub struct Hash<T: HashSize> {
    inner: String,
    _size: PhantomData<T>,
}

impl<T: HashSize> Hash<T> {
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl<T: HashSize> FromStr for Hash<T> {
    type Err = IncorrectHash;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != T::SIZE || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(IncorrectHash);
        }

        Ok(Self {
            inner: s.to_owned(),
            _size: PhantomData,
        })
    }
}
