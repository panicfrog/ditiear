use crate::common::{split_dir_and_name, DiffBlob, DiffBlobType};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::hash::Hasher;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::{fs, io};
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

pub fn create_directory_blob_file<P: AsRef<Path>>(to_path: P, from_path: P) -> io::Result<String> {
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(from_path.as_ref().to_path_buf());
    let mut directories = Vec::new();
    while let Some(p) = queue.pop_front() {
        if p.is_dir() {
            directories.push(p.clone());
            for entry in fs::read_dir(&p)? {
                let entry = entry?;
                let path = entry.path();
                // TODO: 过滤更多隐藏文件
                if path.file_name().unwrap().to_str().unwrap() == ".DS_Store" {
                    continue;
                }
                if path.is_dir() {
                    queue.push_back(path);
                }
            }
        }
    }
    let mut resolved: HashMap<PathBuf, DiffBlob> = HashMap::new();
    while let Some(current_path) = directories.pop() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&current_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.file_name().unwrap().to_str().unwrap() == ".DS_Store" {
                continue;
            }
            if path.is_dir() {
                if let Some(e) = resolved.get(&path) {
                    entries.push(e.clone());
                    resolved.remove(&path);
                }
                // else {
                //     return Err(io::Error::new(io::ErrorKind::Other, "not found"))
                // }
            } else {
                write_file_blob(&to_path, &mut entries, &path)?;
            }
        }
        if entries.is_empty() {
            continue;
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let hash = write_directory_blob(&to_path, &mut entries)?;
        resolved.insert(
            current_path.clone(),
            DiffBlob {
                name: current_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
                hash,
                blob_type: DiffBlobType::Directory,
            },
        );
    }
    resolved
        .get(&from_path.as_ref().to_path_buf())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "not found"))
        .map(|e| e.hash.clone())
}

pub fn create_directory_blob_file_rec<P: AsRef<Path>>(
    to_path: P,
    from_path: P,
) -> io::Result<String> {
    // 1. read directory info, if not a directory return error
    let dir = std::fs::read_dir(from_path)?;

    // 2. walk directory and calculate hash for each file, if is a subdirectory, call create_directory_blob_file recursively
    let mut blobs = Vec::new();
    for entry in dir {
        let entry = entry?;
        let path = entry.path();
        // TODO: 过滤更多隐藏文件
        if path.file_name().unwrap().to_str().unwrap() == ".DS_Store" {
            continue;
        }
        if path.is_dir() {
            let hash = create_directory_blob_file_rec(to_path.as_ref(), path.as_path())?;
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            let blob = DiffBlob {
                name,
                hash,
                blob_type: DiffBlobType::Directory,
            };
            blobs.push(blob);
        } else {
            write_file_blob(&to_path, &mut blobs, &path)?;
        }
    }

    // 3. get all file hashes and sort them by hash
    blobs.sort_by(|a, b| a.name.cmp(&b.name));

    // 4. calculate hash for all file hashes combined
    write_directory_blob(&to_path, &mut blobs)
}

#[inline]
fn write_directory_blob<P: AsRef<Path>>(
    to_path: &P,
    blobs: &mut Vec<DiffBlob>,
) -> io::Result<String> {
    let mut hasher = XxHash64::default();
    for blob in blobs.iter() {
        hasher.write(blob.to_string().as_bytes());
    }
    let hash = format!("{:x}", hasher.finish());
    let (dir, name) = split_dir_and_name(&hash);
    let p = &to_path.as_ref().join(dir);
    if !p.exists() {
        fs::create_dir_all(p)?;
    }
    let p = &p.join(name);
    if p.exists() {
        return Ok(hash);
    } else {
        let mut file = File::create(p)?;
        for blob in blobs.iter() {
            file.write_all(blob.to_string().as_bytes())?;
        }
    }
    Ok(hash)
}

#[inline]
fn write_file_blob<P: AsRef<Path>>(
    to_path: &P,
    entries: &mut Vec<DiffBlob>,
    path: &PathBuf,
) -> io::Result<()> {
    let hash = calculate_file_hash(&path)?;
    let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
    let (dir, name) = split_dir_and_name(&hash);
    let p = &to_path.as_ref().join(dir);
    if !p.exists() {
        fs::create_dir_all(p)?;
    }
    let p = &p.join(name);
    if !p.exists() {
        fs::copy(&path, p)?;
    }
    let blob = DiffBlob {
        name: file_name,
        hash,
        blob_type: DiffBlobType::File,
    };
    entries.push(blob);
    Ok(())
}
