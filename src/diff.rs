use crate::common::{path_from_hash, DeserializeError, DiffBlob, DiffBlobType};
use crate::diff::DiffCollectionType::Modify;
use bytes::Bytes;
use core::fmt;
use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices, Algorithm, DiffOp};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::read_to_string;
use std::path::Path;
use std::str::FromStr;
use std::{fs, io};
use thiserror::Error;

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
pub enum Patch {
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
pub fn calculate_binary_diff(old: Bytes, new: Bytes) -> Vec<Patch> {
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
            } => Patch::Delete {
                old_index: *old_index,
                new_index: *new_index,
                old_value: old.slice(*old_index..*old_index + *old_len),
            },
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => Patch::Add {
                old_index: *old_index,
                new_index: *new_index,
                new_value: new.slice(*new_index..*new_index + *new_len),
            },
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => Patch::Replace {
                old_index: *old_index,
                new_index: *new_index,
                old_value: old.slice(*old_index..*old_index + *old_len),
                new_value: new.slice(*new_index..*new_index + *new_len),
            },
            _ => !unreachable!(),
        })
        .collect()
}

#[derive(Debug)]
pub enum DiffFileType {
    Directory,
    File,
}

impl fmt::Display for DiffFileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DiffFileType::Directory => "Directory",
                DiffFileType::File => "File",
            }
        )
    }
}

#[derive(Debug)]
pub enum DiffCollectionType {
    Add {
        r#type: DiffFileType,
        value: String,
    },
    Delete {
        r#type: DiffFileType,
        value: String,
    },
    Modify {
        r#type: DiffFileType,
        old: String,
        new: String,
    },
}

impl DiffCollectionType {
    #[inline]
    fn movement_unique_hash(&self) -> Option<String> {
        match self {
            Self::Add { value, r#type } | Self::Delete { value, r#type } => {
                Some(format!("{}{}", value, r#type))
            }
            _ => None,
        }
    }

    // fn is_file(&self) -> bool {
    //     match self {
    //         Self::Add { r#type, .. }
    //         | Self::Delete { r#type, .. }
    //         | Self::Modify { r#type, .. } => {
    //             if let DiffFileType::File = r#type {
    //                 true
    //             } else {
    //                 false
    //             }
    //         }
    //     }
    // }
    // fn is_dir(&self) -> bool {
    //     match self {
    //         Self::Add { r#type, .. }
    //         | Self::Delete { r#type, .. }
    //         | Self::Modify { r#type, .. } => {
    //             if let DiffFileType::Directory = r#type {
    //                 true
    //             } else {
    //                 false
    //             }
    //         }
    //     }
    // }
}

#[derive(Error, Debug)]
pub enum FileParseError {
    #[error("I/O error")]
    Io(#[from] io::Error),
    #[error("Parse error")]
    Parse(#[from] DeserializeError),
}

impl DiffBlob {
    #[inline]
    fn unique_name(&self) -> String {
        format!("{}{}", self.name, self.blob_type)
    }
}

pub fn compare_blob_files<P: AsRef<Path>>(
    old_hash: &str,
    new_hash: &str,
    base: P,
) -> Result<Vec<DiffCollectionType>, FileParseError> {
    let mut queue = VecDeque::new();
    queue.push_front((old_hash.to_string(), new_hash.to_string()));
    let mut result = vec![];
    // mark all added files
    let mut add_set = HashSet::new();
    // mark all deleted files
    let mut delete_set = HashSet::new();
    // traverse sub folders using BSF
    while let Some((old, new)) = queue.pop_back() {
        // 1. read old and new blob files
        let old_path = path_from_hash(&old, base.as_ref());
        let old_file = fs::File::open(old_path)?;
        let mut old_blobs = HashMap::new();
        for line in read_to_string(old_file)?.lines() {
            let blob = DiffBlob::from_str(line)?;
            old_blobs.insert(blob.unique_name(), blob);
        }

        let new_path = path_from_hash(&new, base.as_ref());
        let new_file = fs::File::open(new_path)?;
        let mut new_blobs = HashMap::new();
        for line in read_to_string(new_file)?.lines() {
            let blob = DiffBlob::from_str(line)?;
            new_blobs.insert(blob.unique_name(), blob);
        }
        if old == new {
            continue;
        }
        result.push(Modify {
            r#type: DiffFileType::Directory,
            old: old.to_string(),
            new: new.to_string(),
        });
        // 2. compare two blob files and find differences
        for b in old_blobs.values() {
            if let Some(new_b) = new_blobs.get(&b.unique_name()) {
                // if two blobs are the same, skip
                if b.hash == new_b.hash {
                    continue;
                }
                // if two blobs are different and has the same name and type, mark as modified
                if let DiffBlobType::File = b.blob_type {
                    result.push(Modify {
                        r#type: DiffFileType::File,
                        old: b.hash.clone(),
                        new: new_b.hash.clone(),
                    });
                } else {
                    queue.push_front((b.hash.clone(), new_b.hash.clone()));
                }
            } else {
                // if a blob is in old but not in new, mark as deleted
                if let DiffBlobType::File = b.blob_type {
                    let diff_item = DiffCollectionType::Delete {
                        r#type: DiffFileType::File,
                        value: b.hash.clone(),
                    };
                    // it is delete, so unwrap is safe.
                    delete_set.insert(diff_item.movement_unique_hash().unwrap());
                    result.push(diff_item);
                } else {
                    let (subs, set) = walk_dir(
                        base.as_ref(),
                        DiffCollectionType::Delete {
                            r#type: DiffFileType::Directory,
                            value: b.hash.clone(),
                        },
                    )?;
                    delete_set.extend(set);
                    result.extend(subs);
                }
            }
        }
        // modified and deleted files are already marked, so we only need to mark added files
        for b in new_blobs.values() {
            if old_blobs.get(&b.unique_name()).is_some() {
                continue;
            }
            if let DiffBlobType::File = b.blob_type {
                let diff_item = DiffCollectionType::Add {
                    r#type: DiffFileType::File,
                    value: b.hash.clone(),
                };
                // it is add, so unwrap is safe.
                add_set.insert(diff_item.movement_unique_hash().unwrap());
                result.push(diff_item);
            } else {
                let (subs, set) = walk_dir(
                    base.as_ref(),
                    DiffCollectionType::Add {
                        r#type: DiffFileType::Directory,
                        value: b.hash.clone(),
                    },
                )?;
                add_set.extend(set);
                result.extend(subs);
            }
        }
    }
    // 3. filter out invalid files
    // invalid files are files that are both added and deleted
    let invalid_set: HashSet<_> = add_set.intersection(&delete_set).collect();
    let result = result
        .into_iter()
        .filter(|x| {
            // filter out both added and deleted files.
            if let Some(hash) = x.movement_unique_hash() {
                !invalid_set.contains(&hash)
            } else {
                true
            }
        })
        .collect();
    Ok(result)
}

/**
 * walk directory recursively to mark all sub files and directories with specified change type (add or delete), then return a list of DiffCollectionType and a set of hashes of all files.
 */
fn walk_dir<P: AsRef<Path>>(
    base: P,
    diff_collection_type: DiffCollectionType,
) -> Result<(Vec<DiffCollectionType>, HashSet<String>), FileParseError> {
    let mut result = vec![];
    let mut set = HashSet::new();
    let mut stack = vec![];
    let (p, is_add) = match diff_collection_type {
        DiffCollectionType::Add { value, .. } => (value, true),
        DiffCollectionType::Delete { value, .. } => (value, false),
        _ => unreachable!("work_dir"),
    };
    stack.push(p);
    while let Some(hash) = stack.pop() {
        let p = path_from_hash(&hash, base.as_ref());
        let dir_content = fs::read_to_string(&p)?;
        let diff_item = if is_add {
            DiffCollectionType::Add {
                r#type: DiffFileType::Directory,
                value: hash.clone(),
            }
        } else {
            DiffCollectionType::Delete {
                r#type: DiffFileType::Directory,
                value: hash.clone(),
            }
        };
        // only add and delete will marked, so unwrap is safe.
        set.insert(diff_item.movement_unique_hash().unwrap());
        result.push(diff_item);
        for line in dir_content.lines() {
            let blob = DiffBlob::from_str(line)?;
            if let DiffBlobType::File = blob.blob_type {
                let diff_file_item = if is_add {
                    DiffCollectionType::Add {
                        r#type: DiffFileType::File,
                        value: blob.hash.clone(),
                    }
                } else {
                    DiffCollectionType::Delete {
                        r#type: DiffFileType::File,
                        value: blob.hash.clone(),
                    }
                };
                // only add and delete will marked, so unwrap is safe.
                set.insert(diff_file_item.movement_unique_hash().unwrap());
                result.push(diff_file_item);
            } else {
                stack.push(blob.hash);
            }
        }
    }
    Ok((result, set))
}
