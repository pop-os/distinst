use std::fs::File;
use std::io::{self, BufRead, Cursor, Read, Write};
use std::path::Path;

pub(crate) struct EnvFile<'a>(&'a Path);

impl<'a> EnvFile<'a> {
    pub fn new(path: &'a Path) -> EnvFile<'a> {
        EnvFile(path)
    }

    pub fn update(&self, key: &str, value: &str) -> io::Result<()> {
        read(self.0)
            .and_then(|data| replace_env(data, key, value))
            .and_then(|ref new_data| write(self.0, new_data))
    }

    pub fn exists(&self) -> bool { self.0.exists() }
}

fn replace_env(buffer: Vec<u8>, key: &str, value: &str) -> io::Result<Vec<u8>> {
    let mut new_buffer = Vec::with_capacity(buffer.len());
    for line in Cursor::new(buffer).lines() {
        let line = line?;
        if line.starts_with(&[key, "="].concat()) {
            new_buffer.extend_from_slice(key.as_bytes());
            new_buffer.push(b'=');
            new_buffer.extend_from_slice(value.as_bytes());
        } else {
            new_buffer.extend_from_slice(line.as_bytes());
        }
        new_buffer.push(b'\n');
    }

    Ok(new_buffer)
}

// TODO: These will be no longer be required once Rust is updated in the repos to 1.26.0

fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    File::open(path).and_then(|mut file| {
        let mut buffer = Vec::with_capacity(file.metadata().ok().map_or(0, |x| x.len()) as usize);
        file.read_to_end(&mut buffer).map(|_| buffer)
    })
}

fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
    File::create(path).and_then(|mut file| file.write_all(contents.as_ref()))
}
