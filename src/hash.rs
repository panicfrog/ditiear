use std::collections::{HashMap};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::hash::Hasher;
use std::{fs, io};
use std::io::{Read, Write};
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

fn split_dir_and_name(hash: &str) -> (&str, &str) {
    // assert hash length is longer than 2
    let dir = &hash[0..1];
    let name = &hash[1..];
    (dir, name)
}

#[derive(Clone)]
enum DiffBlobType {
    Directory,
    File,
    Patch
}

#[derive(Clone)]
struct DiffBlob {
    name: String,
    hash: String,
    blob_type: DiffBlobType,
}

impl Display for DiffBlob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} {}\n", self.name, self.hash, match self.blob_type {
            DiffBlobType::Directory => "directory",
            DiffBlobType::File => "file",
            DiffBlobType::Patch => "patch",
        })
    }
}

pub fn create_directory_blob_file<P: AsRef<Path>>(to_path: P, from_path: P) -> io::Result<String> {
    let mut entries_stack:Vec<(PathBuf, Vec<DiffBlob>)> = vec![(from_path.as_ref().to_path_buf(), Vec::new())];
    let mut resolved: HashMap<PathBuf, DiffBlob> = HashMap::new();

    while let Some((parent_path, mut entries)) = entries_stack.pop() {
        let mut tem_entries_stack = vec![];
        let mut need_update = false;
        let mut all_resolved = true;
        for entry in fs::read_dir(&parent_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.file_name().unwrap().to_str().unwrap() == ".DS_Store" {
                continue;
            }
            if let Some(e) = resolved.get(&path) {
                continue;
            } else {
                if entry.path().is_dir() {
                    tem_entries_stack.push((path.clone(), Vec::new()));
                    all_resolved = false;
                } else {
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
                    resolved.insert(path.clone(), blob.clone());
                    entries.push(blob);
                    need_update = true;
                }
                if !tem_entries_stack.is_empty() || need_update {
                    entries_stack.push((parent_path.clone(), entries.clone()));
                    if !tem_entries_stack.is_empty() {
                        entries_stack.append(&mut tem_entries_stack);
                    }
                }
            }
        }
        if all_resolved && parent_path.is_dir() && !entries.is_empty() {
            let mut hasher = XxHash64::default();
            entries.sort_by(|a, b| a.hash.cmp(&b.hash));
            for blob in entries.iter() {
                hasher.write(blob.to_string().as_bytes());
            }
            let hash = format!("{:x}", hasher.finish()) ;
            let (dir, name) = split_dir_and_name(&hash);

            let p = &to_path.as_ref().join(dir);
            if !p.exists() {
                fs::create_dir_all(p)?;
            }
            let p = &p.join(name);
            if !p.exists() {
                let mut file = File::create(p)?;
                for blob in entries.iter() {
                    file.write_all(blob.to_string().as_bytes())?;
                }
            }
            resolved.insert(parent_path.clone(), DiffBlob {
                name: parent_path.file_name().unwrap().to_str().unwrap().to_string(),
                hash,
                blob_type: DiffBlobType::Directory,
            });
        }
    }
    return if let Some(e) = resolved.get(&from_path.as_ref().to_path_buf()) {
        Ok(e.hash.clone())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "not found"))
    }
}

pub fn create_directory_blob_file_rec<P: AsRef<Path>>(to_path: P, from_path: P) -> io::Result<String> {
    // 1. read directory info, if not a directory return error
    let dir = std::fs::read_dir(from_path)?;

    // 2. walk directory and calculate hash for each file, if is a subdirectory, call create_directory_blob_file recursively
    let mut blobs = Vec::new();
    for entry in dir {
        let entry = entry?;
        let path = entry.path();
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
            blobs.push(blob);
        }
    }

    // 3. get all file hashes and sort them by hash
    blobs.sort_by(|a, b| a.hash.cmp(&b.hash));

    // 4. calculate hash for all file hashes combined
    let mut hasher = XxHash64::default();
    for blob in blobs.iter() {
        hasher.write(blob.to_string().as_bytes());
    }
    let hash = format!("{:x}", hasher.finish()) ;

    // 5. write the content of the hashes to a file with name of the hash
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
