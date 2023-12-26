#[cfg(feature = "binaryBlob")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "binaryBlob")]
use serde_columnar::{columnar, from_bytes, to_vec};
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

#[derive(Clone, PartialEq, Eq, Debug)]
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

#[cfg(feature = "binaryBlob")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[columnar(vec, ser, de)]
pub struct BinaryDiffBlob {
    #[columnar(strategy = "Rle")]
    pub name: String,
    #[columnar(strategy = "Rle")]
    pub(crate) hash: String,
    #[columnar(strategy = "Rle")]
    pub(crate) name_len: u8,
    #[columnar(strategy = "Rle")]
    pub(crate) hash_len: u8,
    #[columnar(strategy = "Rle")]
    pub(crate) blob_type_len: u8,
}

#[cfg(feature = "binaryBlob")]
#[derive(Debug)]
#[columnar(vec, ser, de)]
pub struct BinaryDiffBlobStore {
    #[columnar(class = "vec")]
    pub blobs: Vec<BinaryDiffBlob>,
}

#[cfg(feature = "binaryBlob")]
impl DiffBlob {
    fn into_binary(self) -> BinaryDiffBlob {
        let DiffBlob {
            name,
            hash,
            blob_type,
        } = self;
        let name_len = name.len();
        let hash_len = hash.len();
        let blob_type_len = blob_type.to_string().len();
        assert!(name_len < 256);
        assert!(hash_len < 256);
        assert!(blob_type_len < 256);
        BinaryDiffBlob {
            name,
            hash,
            name_len: name_len as u8,
            hash_len: hash_len as u8,
            blob_type_len: blob_type_len as u8,
        }
    }
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
        let len = s.len();
        if len < 6 {
            return Err(DeserializeError::InvalidLength);
        }
        let (s, len_part) = s.split_at(len - 6);
        let (name_length, hash_length, type_length) = (
            usize::from_str_radix(&len_part[0..2], 16)
                .map_err(|_| DeserializeError::InvalidNameLengthInfo)?,
            usize::from_str_radix(&len_part[2..4], 16)
                .map_err(|_| DeserializeError::InvalidHashLengthInfo)?,
            usize::from_str_radix(&len_part[4..6], 16)
                .map_err(|_| DeserializeError::InvalidTypeLengthInfo)?,
        );
        if len < name_length + hash_length + type_length + 3 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_blob() {
        let blob = DiffBlob {
            name: "name".to_string(),
            hash: "hash".to_string(),
            blob_type: DiffBlobType::Directory,
        };
        let s = blob.to_string();
        assert_eq!(s, "name hash directory 040409\n");
        let _ = DiffBlob::from_str(&s).unwrap();
        let blob = DiffBlob {
            name: "name".to_string(),
            hash: "hash".to_string(),
            blob_type: DiffBlobType::File,
        };
        let s = blob.to_string();
        assert_eq!(s, "name hash file 040404\n");
    }

    #[cfg(feature = "binaryBlob")]
    #[test]
    fn test_binary_diff_blob() {
        let mut blobs = vec![];
        for i in 0..100 {
            let blob = DiffBlob {
                name: format!("name{}", i),
                hash: format!("hash{}", i),
                blob_type: DiffBlobType::Directory,
            };
            let binary_blob = blob.into_binary();
            blobs.push(binary_blob);
        }
        let blob_store = BinaryDiffBlobStore { blobs };
        let buf = to_vec(&blob_store).unwrap();
        let mut blob_store2 = from_bytes::<BinaryDiffBlobStore>(&buf).unwrap();
        assert_eq!(blob_store2.blobs.len(), 100);
        let a = blob_store2.blobs.pop().unwrap();
        assert_eq!(a.name, "name99");
        println!("{:?}, last hash: {}", buf.len(), a.hash);
    }
}
