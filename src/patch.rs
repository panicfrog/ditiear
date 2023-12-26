use bytes::Bytes;
use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices, Algorithm, DiffOp};

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
pub enum BlobPatch {
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
pub fn calculate_binary_diff(old: Bytes, new: Bytes) -> Vec<BlobPatch> {
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
            } => BlobPatch::Delete {
                old_index: *old_index,
                new_index: *new_index,
                old_value: old.slice(*old_index..*old_index + *old_len),
            },
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => BlobPatch::Add {
                old_index: *old_index,
                new_index: *new_index,
                new_value: new.slice(*new_index..*new_index + *new_len),
            },
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => BlobPatch::Replace {
                old_index: *old_index,
                new_index: *new_index,
                old_value: old.slice(*old_index..*old_index + *old_len),
                new_value: new.slice(*new_index..*new_index + *new_len),
            },
            _ => !unreachable!(),
        })
        .collect()
}
