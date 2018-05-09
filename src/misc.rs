//! An assortment of useful basic functions useful throughout the project.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

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
