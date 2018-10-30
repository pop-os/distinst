//! Libary for parsing environment files into an in-memory map.

extern crate distinst_utils as misc;
#[macro_use]
extern crate log;

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::str;

use misc::{read, write};

pub struct EnvFile<'a> {
    path:  &'a Path,
    store: BTreeMap<String, String>,
}

impl<'a> EnvFile<'a> {
    pub fn new(path: &'a Path) -> io::Result<EnvFile<'a>> {
        let data = read(path)?;
        let mut store = BTreeMap::new();

        let values = data.split(|&x| x == b'\n').flat_map(|entry| {
            entry.iter().position(|&x| x == b'=').and_then(|pos| {
                String::from_utf8(entry[..pos].to_owned()).ok()
                    .and_then(|x| {
                        String::from_utf8(entry[pos+1..].to_owned()).ok().map(|y| (x, y))
                    })
            })
        });

        for (key, value) in values {
            store.insert(key, value);
        }

        Ok(EnvFile { path, store })
    }

    pub fn update(&mut self, key: &str, value: &str) {
        info!("updating {} with {} in env file", key, value);
        self.store.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        info!("getting {} from env file", key);
        self.store.get(key).as_ref().map(|x| x.as_str())
    }

    pub fn write(&mut self) -> io::Result<()> {
        info!("writing recovery changes");
        let mut buffer = Vec::with_capacity(1024);
        for (key, value) in &self.store {
            buffer.extend_from_slice(key.as_bytes());
            buffer.push(b'=');
            buffer.extend_from_slice(value.as_bytes());
            buffer.push(b'\n');
        }

        write(&self.path, &buffer)
    }
}

#[cfg(test)]
mod tests {
    extern crate tempdir;
    use super::*;
    use misc;
    use self::tempdir::TempDir;
    use std::collections::BTreeMap;
    use std::io::Write;

    const SAMPLE: &str = r#"EFI_UUID=DFFD-D047
HOSTNAME=pop-testing
KBD_LAYOUT=us
KBD_MODEL=
KBD_VARIANT=
LANG=en_US.UTF-8
OEM_MODE=0
RECOVERY_UUID=PARTUUID=asdfasd7asdf7sad-asdfa
ROOT_UUID=2ef950c2-5ce6-4ae0-9fb9-a8c7468fa82c
"#;

    #[test]
    fn env_file_read() {
        let tempdir = TempDir::new("distinst_test").unwrap();
        let path = &tempdir.path().join("recovery.conf");

        {
            let mut file = misc::create(path).unwrap();
            file.write_all(SAMPLE.as_bytes()).unwrap();
        }

        let env = EnvFile::new(path).unwrap();
        assert_eq!(&env.store, &{
            let mut map = BTreeMap::new();
            map.insert("HOSTNAME".into(), "pop-testing".into());
            map.insert("LANG".into(), "en_US.UTF-8".into());
            map.insert("KBD_LAYOUT".into(), "us".into());
            map.insert("KBD_MODEL".into(), "".into());
            map.insert("KBD_VARIANT".into(), "".into());
            map.insert("EFI_UUID".into(), "DFFD-D047".into());
            map.insert("RECOVERY_UUID".into(), "PARTUUID=asdfasd7asdf7sad-asdfa".into());
            map.insert("ROOT_UUID".into(), "2ef950c2-5ce6-4ae0-9fb9-a8c7468fa82c".into());
            map.insert("OEM_MODE".into(), "0".into());
            map
        });
    }

    #[test]
    fn env_file_write() {
        let tempdir = TempDir::new("distinst_test").unwrap();
        let path = &tempdir.path().join("recovery.conf");

        {
            let mut file = misc::create(path).unwrap();
            file.write_all(SAMPLE.as_bytes()).unwrap();
        }

        let mut env = EnvFile::new(path).unwrap();
        env.write().unwrap();
        let copy: &[u8] = &misc::read(path).unwrap();

        assert_eq!(copy, SAMPLE.as_bytes());
    }
}
