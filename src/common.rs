use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

pub(crate) fn split_dir_and_name(hash: &str) -> (&str, &str) {
    // assert hash length is longer than 2
    let dir = &hash[0..1];
    let name = &hash[1..];
    (dir, name)
}

pub(crate) fn path_from_hash<P: AsRef<Path>>(hash: &str, base: P) -> PathBuf {
    let (dir, name) = split_dir_and_name(hash);
    base.as_ref().join(dir).join(name)
}

#[derive(Clone)]
pub enum DiffBlobType {
    Directory,
    File,
}

impl Display for DiffBlobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffBlobType::Directory => write!(f, "directory"),
            DiffBlobType::File => write!(f, "file"),
        }
    }
}

#[derive(Clone)]
pub struct DiffBlob {
    pub(crate) name: String,
    pub(crate) hash: String,
    pub(crate) blob_type: DiffBlobType,
}

impl Display for DiffBlob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name_length = self.name.len();
        let hash_length = self.hash.len();
        let type_length = self.blob_type.to_string().len();
        write!(
            f,
            "{} {} {} {:02x}{:02x}{:02x}\n",
            self.name, self.hash, self.blob_type, name_length, hash_length, type_length
        )
    }
}

#[derive(Error, Debug)]
#[error("Deserialize error")]
pub enum DeserializeError {
    InvalidLength,
    InvalidNameLengthInfo,
    InvalidHashLengthInfo,
    InvalidTypeLengthInfo,
    InvalidTotalLength,
    InvalidType,
}

impl FromStr for DiffBlob {
    type Err = DeserializeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.len() < 6 {
            return Err(DeserializeError::InvalidLength);
        }
        let (len_part, s) = s.split_at(6);
        let (name_length, hash_length, type_length) = (
            usize::from_str_radix(&len_part[0..2], 16)
                .map_err(|_| DeserializeError::InvalidNameLengthInfo)?,
            usize::from_str_radix(&len_part[2..4], 16)
                .map_err(|_| DeserializeError::InvalidHashLengthInfo)?,
            usize::from_str_radix(&len_part[4..6], 16)
                .map_err(|_| DeserializeError::InvalidTypeLengthInfo)?,
        );
        if s.len() < name_length + hash_length + type_length + 3 {
            return Err(DeserializeError::InvalidTotalLength);
        }
        let (name, rest) = s.split_at(name_length);
        let (hash, rest) = rest[1..].split_at(hash_length);
        let blob_type_str = rest[1..].split_at(type_length).0;
        let blob_type = match blob_type_str {
            "directory" => DiffBlobType::Directory,
            "file" => DiffBlobType::File,
            _ => return Err(DeserializeError::InvalidType),
        };
        Ok(DiffBlob {
            name: name.to_string(),
            hash: hash.to_string(),
            blob_type,
        })
    }
}
