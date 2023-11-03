use rcodepush::prelude::*;

#[test]
fn test_calculate_binary_diff() {
    let v1 = vec![1, 2, 3, 4, 5];
    let v2 = vec![1, 2, 3, 4, 6];
    let ops = calculate_binary_diff(&v1, &v2);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0], Patch::Replace { old_index: 4, new_index: 4, old_value: vec![5], new_value: vec![6] });
}

#[test]
fn test_calculate_file_hash() {
    match calculate_file_hash("./tests/choose_new_idcard.webp") {
        Ok(h) => assert_eq!("19127e790ea4e3ea", h),
        Err(e) => panic!("{:?}", e)
    }
}