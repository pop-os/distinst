use std::io::{self, BufRead, Cursor};
use std::path::Path;
use std::str;

use misc::{read, write};

pub(crate) struct EnvFile<'a>(&'a Path);

impl<'a> EnvFile<'a> {
    pub fn new(path: &'a Path) -> EnvFile<'a> {
        EnvFile(path)
    }

    pub fn update(&self, key: &str, value: &str) -> io::Result<()> {
        info!("libdistinst: updating {} with {} in env file", key, value);
        read(self.0)
            .and_then(|data| replace_env(data, key, value))
            .and_then(|ref new_data| write(self.0, new_data))
    }

    pub fn get(&self, key: &str) -> io::Result<String> {
        info!("libdistinst: getting {} from env file", key);
        read(self.0).and_then(|data| get_env(data, key))
    }

    pub fn exists(&self) -> bool { self.0.exists() }
}

fn get_env(buffer: Vec<u8>, key: &str) -> io::Result<String> {
    for line in Cursor::new(buffer).lines() {
        let line = line?;
        if line.starts_with(&[key, "="].concat()) {
            return Ok(line[key.len() + 1..].into())
        }
    }

    Err(io::Error::new(io::ErrorKind::NotFound, "key not found in env file"))
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
