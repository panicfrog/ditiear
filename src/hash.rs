use std::path::Path;
use std::fs::File;
use std::hash::Hasher;
use std::io;
use std::io::Read;
use twox_hash::XxHash64;

pub fn calculate_file_hash<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = XxHash64::default();
    let mut buffer = [0; 1024];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.write(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finish()))
}