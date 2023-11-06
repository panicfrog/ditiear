use similar::{Algorithm, capture_diff_slices, DiffOp};
use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum Patch {
    Add{old_index: usize, new_index: usize, new_value: Vec<u8> },
    Delete{old_index: usize, new_index: usize, old_value: Vec<u8>},
    Replace{old_index: usize, new_index: usize, old_value: Vec<u8>, new_value: Vec<u8>},
}

#[allow(unreachable_code)]
pub fn calculate_binary_diff(old: &[u8], new: &[u8]) -> Vec<Patch>{
    let ops = capture_diff_slices(Algorithm::Myers, old, new  );
    ops.iter().filter(|op| match op {
        DiffOp::Equal{..} => false,
        _ => true
    }).map(|op| {
        match op {
            DiffOp::Delete{old_index,  old_len, new_index} => {
                Patch::Delete {
                    old_index: *old_index,
                    new_index: *new_index,
                    old_value: old[*old_index..*old_index+*old_len].to_vec(),}
            },
            DiffOp::Insert{old_index, new_index, new_len} => {
                Patch::Add {
                    old_index: *old_index,
                    new_index: *new_index,
                    new_value: new[*new_index..*new_index+*new_len].to_vec(),}
            },
            DiffOp::Replace{old_index,old_len, new_index,new_len} => {
                Patch::Replace {
                    old_index: *old_index,
                    new_index: *new_index,
                    old_value: old[*old_index..*old_index+*old_len].to_vec(),
                    new_value: new[*new_index..*new_index+*new_len].to_vec(),}
            },
            _ => !unreachable!()
        }
    }).collect()
}