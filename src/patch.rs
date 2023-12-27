use bytes::Bytes;
use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices, Algorithm, DiffOp};
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::{fs, path::Path};
use thiserror::Error;
use zip::write::{FileOptions, ZipWriter};
use zip::{CompressionMethod, ZipArchive};

use crate::common::DeserializeError;
use crate::{
    common::{path_from_hash, FileParseError},
    diff::DiffCollectionType,
};

fn serialize_bytes<S>(bytes: &Bytes, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serde_bytes::serialize(bytes.as_ref(), serializer)
}

fn deserialize_bytes<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    Ok(Bytes::from(bytes))
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum BytesPatch {
    Add {
        old_index: usize,
        new_index: usize,
        #[serde(
            serialize_with = "serialize_bytes",
            deserialize_with = "deserialize_bytes"
        )]
        new_value: Bytes,
    },
    Delete {
        old_index: usize,
        new_index: usize,
        #[serde(
            serialize_with = "serialize_bytes",
            deserialize_with = "deserialize_bytes"
        )]
        old_value: Bytes,
    },
    Replace {
        old_index: usize,
        new_index: usize,
        #[serde(
            serialize_with = "serialize_bytes",
            deserialize_with = "deserialize_bytes"
        )]
        old_value: Bytes,
        #[serde(
            serialize_with = "serialize_bytes",
            deserialize_with = "deserialize_bytes"
        )]
        new_value: Bytes,
    },
}

#[allow(unreachable_code)]
pub fn calculate_binary_diff(old: Bytes, new: Bytes) -> Vec<BytesPatch> {
    let ops = capture_diff_slices(Algorithm::Myers, old.as_ref(), new.as_ref());
    ops.iter()
        .filter(|op| match op {
            DiffOp::Equal { .. } => false,
            _ => true,
        })
        .map(|op| match op {
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => BytesPatch::Delete {
                old_index: *old_index,
                new_index: *new_index,
                old_value: old.slice(*old_index..*old_index + *old_len),
            },
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => BytesPatch::Add {
                old_index: *old_index,
                new_index: *new_index,
                new_value: new.slice(*new_index..*new_index + *new_len),
            },
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => BytesPatch::Replace {
                old_index: *old_index,
                new_index: *new_index,
                old_value: old.slice(*old_index..*old_index + *old_len),
                new_value: new.slice(*new_index..*new_index + *new_len),
            },
            _ => !unreachable!(),
        })
        .collect()
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum BlobPatch {
    Add {
        new_file: String,
    },
    Delete {
        old_file: String,
    },
    Replace {
        old_file: String,
        new_file: String,
        patch: Vec<BytesPatch>,
    },
}

impl BlobPatch {
    fn from<T, P>(diffs: T, base_path: P) -> Result<Vec<BlobPatch>, FileParseError>
    where
        T: IntoIterator<Item = DiffCollectionType>,
        P: AsRef<Path>,
    {
        let mut result = vec![];
        for diff in diffs {
            match diff {
                DiffCollectionType::Add { value, .. } => {
                    result.push(BlobPatch::Add { new_file: value })
                }
                DiffCollectionType::Delete { value, .. } => {
                    result.push(BlobPatch::Delete { old_file: value })
                }
                DiffCollectionType::Modify { old, new, .. } => {
                    let old_buffer = bytes_from(&old, base_path.as_ref())?;
                    let new_buffer = bytes_from(&new, base_path.as_ref())?;
                    let patch = calculate_binary_diff(old_buffer, new_buffer);
                    result.push(BlobPatch::Replace {
                        old_file: old,
                        new_file: new,
                        patch,
                    })
                }
            }
        }
        Ok(result)
    }
}

fn bytes_from<P: AsRef<Path>>(hash: &str, base_path: P) -> Result<Bytes, FileParseError> {
    let old_path = path_from_hash(hash, base_path.as_ref());
    let mut old_file = fs::File::open(old_path)?;
    let mut old_buffer = Vec::new();
    old_file.read_to_end(&mut old_buffer)?;
    Ok(Bytes::from(old_buffer))
}

#[derive(Error, Debug)]
pub enum ZipFileError {
    #[error("I/O error")]
    Io(#[from] io::Error),
    #[error("Parse error")]
    Parse(#[from] DeserializeError),
    #[error("Parse error")]
    Serialize(#[from] bincode::Error),
    #[error("Zip error")]
    Zip(#[from] zip::result::ZipError),
}

impl From<FileParseError> for ZipFileError {
    fn from(e: FileParseError) -> Self {
        match e {
            FileParseError::Io(e) => ZipFileError::Io(e),
            FileParseError::Parse(e) => ZipFileError::Parse(e),
        }
    }
}

pub fn create_zip_patch<T, P>(diffs: T, from_dir: P, to_dest: P) -> Result<(), ZipFileError>
where
    T: IntoIterator<Item = DiffCollectionType>,
    P: AsRef<Path>,
{
    let patchs = BlobPatch::from(diffs, from_dir.as_ref())?;
    if patchs.is_empty() {
        return Ok(());
    }
    let zip_file = fs::File::create(to_dest)?;
    let mut zip = ZipWriter::new(zip_file);
    zip.start_file(
        "ditiear.patch",
        FileOptions::default().compression_method(CompressionMethod::Deflated),
    )?;
    let mut add_patchs = vec![];
    for p in patchs {
        let serialized = bincode::serialize(&p)?;
        zip.write_all(&serialized)?;
        match p {
            BlobPatch::Add { .. } => {
                add_patchs.push(p);
            }
            _ => {}
        }
    }
    for p in add_patchs {
        match p {
            BlobPatch::Add { new_file } => {
                let bytes = bytes_from(&new_file, from_dir.as_ref())?;
                zip.start_file(
                    new_file,
                    FileOptions::default().compression_method(CompressionMethod::Deflated),
                )?;
                zip.write_all(&bytes)?;
            }
            _ => {}
        }
    }
    zip.finish()?;
    Ok(())
}

pub fn unpack_patch<P: AsRef<Path>, F>(
    patch_path: P,
    process_file: F,
) -> Result<Vec<BlobPatch>, ZipFileError>
where
    F: Fn(Vec<u8>, &str) -> Result<(), io::Error>,
{
    let zip_file = fs::File::open(patch_path)?;
    let mut archive = ZipArchive::new(zip_file)?;

    let mut patchs = vec![];
    for i in 0..archive.len() {
        let mut file: zip::read::ZipFile<'_> = archive.by_index(i)?;
        if file.name() != "ditiear.patch" {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            process_file(buf, file.name())?;
            continue;
        }
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        let patch: BlobPatch = bincode::deserialize(&buffer)?;
        patchs.push(patch);
    }
    Ok(patchs)
}

pub fn apply_patchs<P: AsRef<Path>>(patch_path: P, base_path: P) -> Result<(), ZipFileError> {
    let patchs = unpack_patch(patch_path, |buffer, name| {
        let path = path_from_hash(name, base_path.as_ref());
        let mut file = fs::File::create(path)?;
        file.write_all(&buffer)
    })?;
    for patch in patchs {
        match patch {
            BlobPatch::Add { new_file } => {
                // TODO: Check if file exists
            }
            BlobPatch::Delete { old_file } => {
                // TODO: Check whether file need to be deleted according to settings
                // let path = path_from_hash(&old_file, base_path.as_ref());
                // fs::remove_file(path)?;
            }
            BlobPatch::Replace {
                old_file,
                new_file,
                patch,
            } => {
                let mut replacements = patch
                    .into_iter()
                    .map(|item| match item {
                        BytesPatch::Add {
                            old_index,
                            new_value,
                            ..
                        } => Replacement {
                            start: old_index,
                            length: 0,
                            content: new_value,
                        },
                        BytesPatch::Delete {
                            old_index,
                            old_value,
                            ..
                        } => Replacement {
                            start: old_index,
                            length: old_value.len(),
                            content: Bytes::new(),
                        },
                        BytesPatch::Replace {
                            old_index,
                            old_value,
                            new_value,
                            ..
                        } => Replacement {
                            start: old_index,
                            length: old_value.len(),
                            content: new_value,
                        },
                    })
                    .collect::<Vec<_>>();
                replacements.sort_by(|a, b| a.start.cmp(&b.start));
                let old_path = path_from_hash(&old_file, base_path.as_ref());
                let old_bak_path =
                    path_from_hash(&format!("{}.bak", &old_file), base_path.as_ref());
                replace_parts_file(old_path.as_path(), old_bak_path.as_path(), &replacements)?;
                let new_path = path_from_hash(&new_file, base_path.as_ref());
                fs::rename(old_bak_path.as_path(), new_path.as_path())?;
            }
        }
    }
    Ok(())
}

struct Replacement {
    start: usize,
    length: usize,
    content: Bytes,
}

fn copy_with_buffer<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    mut bytes: usize,
) -> io::Result<()> {
    let mut buffer = vec![0; 1024.min(bytes)];
    while bytes > 0 {
        let len = reader.read(&mut buffer)?;
        writer.write_all(&buffer[..len])?;
        bytes = bytes.saturating_sub(len);
    }
    Ok(())
}

fn replace_parts_file<P: AsRef<Path>, Q: AsRef<Path>>(
    original_file: P,
    dest_file: Q,
    replacements: &[Replacement],
) -> io::Result<()> {
    let original = OpenOptions::new().read(true).open(original_file.as_ref())?;
    let dest = OpenOptions::new()
        .write(true)
        .create(true)
        .open(dest_file.as_ref())?;

    let mut reader = io::BufReader::new(original);
    let mut writer = io::BufWriter::new(dest);

    let mut current_pos = 0;
    for replacement in replacements {
        let bytes_to_copy = replacement.start - current_pos;
        // avoid high memory usage
        copy_with_buffer(&mut reader, &mut writer, bytes_to_copy)?;
        // avoid copying 0 bytes
        if !replacement.content.is_empty() {
            writer.write_all(replacement.content.as_ref())?;
        }
        reader.seek(SeekFrom::Current(replacement.length as i64))?;
        current_pos = replacement.start + replacement.length;
    }
    io::copy(&mut reader, &mut writer)?;
    writer.flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_blob_patch() {
        use crate::patch::BlobPatch;
        use crate::patch::BytesPatch;
        use bytes::Bytes;
        let old = Bytes::from("hello world");
        let new = Bytes::from("hello world!");
        let mut ops = super::calculate_binary_diff(old, new);
        assert_eq!(
            ops,
            vec![BytesPatch::Add {
                old_index: 11,
                new_index: 11,
                new_value: Bytes::from("!"),
            }]
        );
        let old2 = Bytes::from("你好，世界");
        let new2 = Bytes::from("你好，世界！");
        let ops2 = super::calculate_binary_diff(old2, new2);
        ops.extend(ops2);
        let patch = BlobPatch::Replace {
            old_file: "hello.txt".to_string(),
            new_file: "hello.txt".to_string(),
            patch: ops,
        };
        let serialized = bincode::serialize(&patch).unwrap();
        let deserialized: BlobPatch = bincode::deserialize(&serialized).unwrap();
        match deserialized {
            BlobPatch::Replace {
                old_file,
                new_file,
                patch,
            } => {
                assert_eq!(old_file, "hello.txt");
                assert_eq!(new_file, "hello.txt");
                assert_eq!(patch.len(), 2);
            }
            _ => unreachable!(),
        }
        // assert_eq!(patch, deserialized);
    }
}
