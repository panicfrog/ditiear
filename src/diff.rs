use crate::common::{path_from_hash, DeserializeError, DiffBlob, DiffBlobType};
use crate::diff::DiffCollectionType::Modify;
use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices, Algorithm, DiffOp};
use std::collections::{HashMap, VecDeque};
use std::io::read_to_string;
use std::path::Path;
use std::str::FromStr;
use std::{fs, io};
use thiserror::Error;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum Patch {
    Add {
        old_index: usize,
        new_index: usize,
        new_value: Vec<u8>,
    },
    Delete {
        old_index: usize,
        new_index: usize,
        old_value: Vec<u8>,
    },
    Replace {
        old_index: usize,
        new_index: usize,
        old_value: Vec<u8>,
        new_value: Vec<u8>,
    },
}

#[allow(unreachable_code)]
pub fn calculate_binary_diff(old: &[u8], new: &[u8]) -> Vec<Patch> {
    let ops = capture_diff_slices(Algorithm::Myers, old, new);
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
                old_value: old[*old_index..*old_index + *old_len].to_vec(),
            },
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => Patch::Add {
                old_index: *old_index,
                new_index: *new_index,
                new_value: new[*new_index..*new_index + *new_len].to_vec(),
            },
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => Patch::Replace {
                old_index: *old_index,
                new_index: *new_index,
                old_value: old[*old_index..*old_index + *old_len].to_vec(),
                new_value: new[*new_index..*new_index + *new_len].to_vec(),
            },
            _ => !unreachable!(),
        })
        .collect()
}

#[derive(Debug)]
pub enum DiffFileType {
    Directory(String),
    File(String),
}

#[derive(Debug)]
pub enum DiffCollectionType {
    Add(DiffFileType),
    Delete(DiffFileType),
    Modify {
        old: DiffFileType,
        new: DiffFileType,
    },
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
            old: DiffFileType::Directory(old.to_string()),
            new: DiffFileType::Directory(new.to_string()),
        });
        // 2. compare two blob files and find differences
        for b in old_blobs.values() {
            if let Some(new_b) = new_blobs.get(&b.unique_name()) {
                if b.hash == new_b.hash {
                    continue;
                }
                if let DiffBlobType::File = b.blob_type {
                    result.push(Modify {
                        old: DiffFileType::File(b.hash.clone()),
                        new: DiffFileType::File(new_b.hash.clone()),
                    });
                } else {
                    queue.push_front((b.hash.clone(), new_b.hash.clone()));
                }
            } else {
                if let DiffBlobType::File = b.blob_type {
                    result.push(DiffCollectionType::Delete(DiffFileType::File(
                        b.hash.clone(),
                    )));
                } else {
                    result.push(DiffCollectionType::Delete(DiffFileType::Directory(
                        b.hash.clone(),
                    )));
                    let subs = walk_dir(
                        base.as_ref(),
                        DiffCollectionType::Delete(DiffFileType::Directory(b.hash.clone())),
                    )?;
                    result.extend(subs);
                }
            }
        }
        for b in new_blobs.values() {
            if old_blobs.get(&b.unique_name()).is_some() {
                continue;
            }
            if let DiffBlobType::File = b.blob_type {
                result.push(DiffCollectionType::Add(DiffFileType::File(b.hash.clone())));
            } else {
                result.push(DiffCollectionType::Add(DiffFileType::Directory(
                    b.hash.clone(),
                )));
                let subs = walk_dir(
                    base.as_ref(),
                    DiffCollectionType::Add(DiffFileType::Directory(b.hash.clone())),
                )?;
                result.extend(subs);
            }
        }
    }
    Ok(result)
}

fn walk_dir<P: AsRef<Path>>(
    base: P,
    diff_collection_type: DiffCollectionType,
) -> Result<Vec<DiffCollectionType>, FileParseError> {
    let mut result = vec![];
    let mut stack = vec![];
    let (p, is_add) = match diff_collection_type {
        DiffCollectionType::Add(DiffFileType::Directory(hash)) => (hash, true),
        DiffCollectionType::Delete(DiffFileType::Directory(hash)) => (hash, false),
        _ => unreachable!("work_dir"),
    };
    stack.push(p);
    while let Some(hash) = stack.pop() {
        let p = path_from_hash(&hash, base.as_ref());
        let dir_content = fs::read_to_string(&p)?;
        if is_add {
            result.push(DiffCollectionType::Add(DiffFileType::Directory(
                hash.clone(),
            )));
        } else {
            result.push(DiffCollectionType::Delete(DiffFileType::Directory(
                hash.clone(),
            )));
        }
        for line in dir_content.lines() {
            let blob = DiffBlob::from_str(line)?;
            if let DiffBlobType::File = blob.blob_type {
                if is_add {
                    result.push(DiffCollectionType::Add(DiffFileType::File(
                        blob.hash.clone(),
                    )));
                } else {
                    result.push(DiffCollectionType::Delete(DiffFileType::File(
                        blob.hash.clone(),
                    )));
                }
            } else {
                stack.push(blob.hash);
            }
        }
    }
    Ok(result)
}
