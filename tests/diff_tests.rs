use ditiear::prelude::*;

#[test]
fn test_calculate_binary_diff() {
    let v1 = bytes::Bytes::from(vec![1, 2, 3, 4, 5]);
    let v2 = bytes::Bytes::from(vec![1, 2, 3, 4, 6]);
    let ops = calculate_binary_diff(v1.clone(), v2.clone());
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0],
        BytesPatch::Replace {
            old_index: 4,
            new_index: 4,
            old_value: v1.slice(4..5),
            new_value: v2.slice(4..5),
        }
    );
}

#[test]
fn test_calculate_file_hash() {
    match calculate_file_hash("./tests/choose_new_idcard.webp") {
        Ok(h) => assert_eq!("19127e790ea4e3ea", h),
        Err(e) => panic!("{:?}", e),
    }
}

#[test]
fn test_create_directory_blob_file_rec() {
    match create_directory_blob_file_rec("./tests/assets_blobs_rec", "./tests/assets") {
        Ok(h) => assert_eq!("9de80ac230b1e976", h),
        Err(e) => panic!("{:?}", e),
    }
}

#[test]
fn test_create_directory_blob_file() {
    match create_directory_blob_file("./tests/assets_blobs2", "./tests/assets") {
        // Ok(h) => assert_eq!("13c333f81a961f9f", h),
        Ok(h) => println!("{}", h),
        Err(e) => panic!("{:?}", e),
    }
}

#[test]
fn test_create_directory_blob_file2() {
    match create_directory_blob_file("./tests/assets_blobs2", "./tests/assets2") {
        // Ok(h) => assert_eq!("90b8ab5339628e4", h),
        Ok(h) => println!("{}", h),
        Err(e) => panic!("{:?}", e),
    }
}

#[test]
fn test_compare_blob_files() {
    match compare_blob_files(
        "1da38600711c8713",
        "c408a3680edbbf17",
        "./tests/assets_blobs2",
    ) {
        Ok(h) => println!("{:?}", h),
        // Ok(h) => println!("{}", h),
        Err(e) => panic!("{:?}", e),
    }
}

#[test]
fn test_zip_patch() {
    create_diff_patch(
        "1da38600711c8713",
        "c408a3680edbbf17",
        "./tests/assets_blobs2",
        "./tests/test_assets2_patch.zip",
    )
    .unwrap();
}
