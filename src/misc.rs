//! An assortment of useful basic functions useful throughout the project.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Obtains the UUID of the given device path by resolving symlinks in `/dev/disk/by-uuid`
/// until the device is found.
pub fn get_uuid(path: &Path) -> Option<String> {
    let uuid_dir = Path::new("/dev/disk/by-uuid")
        .read_dir()
        .expect("unable to find /dev/disk/by-uuid");

    if let Ok(path) = path.canonicalize() {
        for uuid_entry in uuid_dir.filter_map(|entry| entry.ok()) {
            if let Ok(ref uuid_path) = uuid_entry.path().canonicalize() {
                if uuid_path == &path {
                    if let Some(uuid_entry) = uuid_entry.file_name().to_str() {
                        return Some(uuid_entry.into());
                    }
                }
            }
        }
    }

    None
}

pub fn from_uuid(uuid: &str) -> Option<PathBuf> {
    let uuid_dir = Path::new("/dev/disk/by-uuid")
        .read_dir()
        .expect("unable to find /dev/disk/by-uuid");

    for uuid_entry in uuid_dir.filter_map(|entry| entry.ok()) {
        let uuid_entry = uuid_entry.path();
        if let Some(name) = uuid_entry.file_name() {
            if name == uuid {
                if let Ok(uuid_entry) = uuid_entry.canonicalize() {
                    return Some(uuid_entry);
                }
            }
        }
    }

    None
}

// TODO: These will be no longer be required once Rust is updated in the repos to 1.26.0

pub fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    File::open(path).and_then(|mut file| {
        let mut buffer = Vec::with_capacity(file.metadata().ok().map_or(0, |x| x.len()) as usize);
        file.read_to_end(&mut buffer).map(|_| buffer)
    })
}

pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
    File::create(path).and_then(|mut file| file.write_all(contents.as_ref()))
}
