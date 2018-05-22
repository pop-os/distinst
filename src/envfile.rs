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

#[cfg(test)]
mod tests {
    use super::*;
    use misc;
    use tempdir::TempDir;
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::io::Write;

    const SAMPLE: &str = r#"EFI_UUID=DFFD-D047
HOSTNAME=pop-testing
KBD_LAYOUT=us
KBD_MODEL=
KBD_VARIANT=
LANG=en_US.UTF-8
OEM_MODE=0
RECOVERY_UUID=8DB5-AFF3
ROOT_UUID=2ef950c2-5ce6-4ae0-9fb9-a8c7468fa82c
"#;

    #[test]
    fn env_file_read() {
        let tempdir = TempDir::new("distinst_test").unwrap();
        let path = &tempdir.path().join("recovery.conf");

        {
            let mut file = File::create(path).unwrap();
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
            map.insert("RECOVERY_UUID".into(), "8DB5-AFF3".into());
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
            let mut file = File::create(path).unwrap();
            file.write_all(SAMPLE.as_bytes()).unwrap();
        }

        let mut env = EnvFile::new(path).unwrap();
        env.write().unwrap();
        let copy: &[u8] = &misc::read(path).unwrap();

        assert_eq!(copy, SAMPLE.as_bytes());
    }
}
