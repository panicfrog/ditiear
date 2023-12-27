use std::path::Path;

pub use crate::diff::compare_blob_files;
pub use crate::hash::{
    calculate_file_hash, create_directory_blob_file, create_directory_blob_file_rec,
};
use crate::patch::ZipFileError;
pub use crate::patch::{
    apply_patchs, calculate_binary_diff, create_zip_patch, unpack_patch, BytesPatch,
};

/// Create a patch file from two blobs
pub fn create_diff_patch<P: AsRef<Path>>(
    old: &str,
    new: &str,
    from_dir: P,
    to_dest: P,
) -> Result<(), ZipFileError> {
    let diffs = compare_blob_files(old, new, from_dir.as_ref())?;
    create_zip_patch(diffs, from_dir, to_dest)
}
