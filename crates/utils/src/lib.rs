//! Miscellanious functions used by distinst and its crates.

extern crate sedregex;

use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

pub fn open<P: AsRef<Path>>(path: P) -> io::Result<File> {
    File::open(&path).map_err(|why| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("unable to open file at {:?}: {}", path.as_ref(), why),
        )
    })
}

pub fn create<P: AsRef<Path>>(path: P) -> io::Result<File> {
    File::create(&path).map_err(|why| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("unable to create file at {:?}: {}", path.as_ref(), why),
        )
    })
}

pub fn cp<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> io::Result<u64> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    io::copy(&mut open(src)?, &mut create(dst)?).map_err(|why| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to copy {:?} to {:?}: {}", src, dst, why),
        )
    })
}

pub fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    open(path).and_then(|mut file| {
        let mut buffer = Vec::with_capacity(file.metadata().ok().map_or(0, |x| x.len()) as usize);
        file.read_to_end(&mut buffer).map(|_| buffer)
    })
}

pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
    create(path).and_then(|mut file| file.write_all(contents.as_ref()))
}

pub use self::layout::*;
use sedregex::find_and_replace;
use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    ffi::{OsStr, OsString},
    fs::{self, DirEntry},
    hash::{Hash, Hasher},
    path::PathBuf,
};

mod layout {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
        path::Path,
    };

    pub fn device_layout_hash() -> u64 {
        let hasher = &mut DefaultHasher::new();
        if let Ok(dir) = Path::new("/dev/").read_dir() {
            for entry in dir {
                if let Ok(entry) = entry {
                    entry.path().hash(hasher);

                    if let Ok(md) = entry.metadata() {
                        if let Ok(created) = md.created() {
                            created.hash(hasher);
                        }
                    }
                }
            }
        }

        hasher.finish()
    }
}

pub fn hasher<T: Hash>(key: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

pub fn canonicalize(path: &Path) -> Cow<Path> {
    if let Ok(mut new) = path.canonicalize() {
        while let Ok(tmp) = new.canonicalize() {
            if new == tmp {
                break;
            }
            new = tmp;
        }
        Cow::Owned(new)
    } else {
        Cow::Borrowed(path)
    }
}

/// Concatenates an array of `&OsStr` into a new `OsString`.
pub fn concat_osstr(input: &[&OsStr]) -> OsString {
    let mut output = OsString::with_capacity(input.iter().fold(0, |acc, c| acc + c.len()));

    input.iter().for_each(|comp| output.push(comp));
    output
}

pub fn device_maps<F: FnMut(&Path)>(mut action: F) {
    read_dirs("/dev/mapper", |pv| action(&pv.path())).unwrap()
}

pub fn read_dirs<P: AsRef<Path>, F: FnMut(DirEntry)>(path: P, mut action: F) -> io::Result<()> {
    for entry in path.as_ref().read_dir()? {
        match entry {
            Ok(entry) => action(entry),
            Err(_) => continue,
        }
    }

    Ok(())
}

pub fn resolve_slave(name: &str) -> Option<PathBuf> {
    let slaves_dir = PathBuf::from(["/sys/class/block/", name, "/slaves/"].concat());
    if !slaves_dir.exists() {
        return Some(PathBuf::from(["/dev/", name].concat()));
    }

    let mut slaves = Vec::new();

    for entry in slaves_dir.read_dir().ok()? {
        if let Ok(entry) = entry {
            if let Ok(name) = entry.file_name().into_string() {
                slaves.push(name);
            }
        }
    }

    if slaves.len() == 1 {
        return Some(PathBuf::from(["/dev/", &slaves[0]].concat()));
    }

    None
}

pub fn resolve_to_physical(name: &str) -> Option<PathBuf> {
    let mut physical: Option<PathBuf> = None;

    loop {
        let physical_c = physical.clone();
        let name = physical_c
            .as_ref()
            .map_or(name, |physical| physical.file_name().unwrap().to_str().unwrap());
        if let Some(slave) = resolve_slave(name) {
            if physical.as_ref().map_or(true, |rec| rec != &slave) {
                physical = Some(slave);
                continue;
            }
        }
        break;
    }

    physical
}

pub fn resolve_parent(name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir("/sys/block").ok()? {
        if let Ok(entry) = entry {
            if let Some(file) = entry.file_name().to_str() {
                if name.starts_with(file) {
                    return Some(PathBuf::from(["/dev/", file].concat()));
                }
            }
        }
    }

    None
}

/// Apply sed expressions on a file, and overwrite it if there was a change.
pub fn sed<P: AsRef<Path>>(path: P, pattern: &str) -> io::Result<()> {
    let path = path.as_ref();
    let sources = String::from_utf8(read(path)?).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, format!("{:?} contains non-UTF-8 data", path))
    })?;

    let replace = find_and_replace(&sources, &[pattern]).map_err(|why| {
        io::Error::new(io::ErrorKind::Other, format!("sedregex failure: {:?}", why))
    })?;

    match replace {
        Cow::Borrowed(_) => Ok(()),
        Cow::Owned(text) => write(&path, &text),
    }
}
