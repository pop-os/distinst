use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::str;

use misc::{read, write};

pub(crate) struct EnvFile<'a> {
    path:  &'a Path,
    store: BTreeMap<String, String>,
}

impl<'a> EnvFile<'a> {
    pub fn new(path: &'a Path) -> io::Result<EnvFile<'a>> {
        let data = read(path)?;
        let mut store = BTreeMap::new();

        let values = data.split(|&x| x == b'\n').flat_map(|entry| {
            let fields = &mut entry.split(|&x| x == b'=');
            fields
                .next()
                .and_then(|x| String::from_utf8(x.to_owned()).ok())
                .and_then(|x| {
                    fields
                        .next()
                        .and_then(|x| String::from_utf8(x.to_owned()).ok())
                        .map(|y| (x, y))
                })
        });

        for (key, value) in values {
            store.insert(key, value);
        }

        Ok(EnvFile { path, store })
    }

    pub fn update(&mut self, key: &str, value: &str) {
        info!("libdistinst: updating {} with {} in env file", key, value);
        self.store.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        info!("libdistinst: getting {} from env file", key);
        self.store.get(key).as_ref().map(|x| x.as_str())
    }

    pub fn write(&mut self) -> io::Result<()> {
        info!("libdistinst: writing recovery changes");
        let mut buffer = Vec::with_capacity(1024);
        for (key, value) in self.store.iter() {
            buffer.extend_from_slice(key.as_bytes());
            buffer.push(b'=');
            buffer.extend_from_slice(value.as_bytes());
            buffer.push(b'\n');
        }

        write(&self.path, &buffer)
    }
}
