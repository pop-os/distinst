use std::char;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader, Error, ErrorKind, Result};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// A mount entry which contains information regarding how and where a device
/// is mounted.
pub(crate) struct MountInfo {
    pub(crate) source: PathBuf,
    pub(crate) dest:   PathBuf,
}

pub(crate) struct Mounts(Vec<MountInfo>);

impl Mounts {
    fn parse_value(value: &str) -> Result<OsString> {
        let mut ret = Vec::new();

        let mut bytes = value.bytes();
        while let Some(b) = bytes.next() {
            match b {
                b'\\' => {
                    let mut code = 0;
                    for _i in 0..3 {
                        if let Some(b) = bytes.next() {
                            code *= 8;
                            code += u32::from_str_radix(&(b as char).to_string(), 8)
                                .map_err(|err| Error::new(ErrorKind::Other, err))?;
                        } else {
                            return Err(Error::new(ErrorKind::Other, "truncated octal code"));
                        }
                    }
                    ret.push(code as u8);
                }
                _ => {
                    ret.push(b);
                }
            }
        }

        Ok(OsString::from_vec(ret))
    }

    fn parse_line(line: &str) -> Result<MountInfo> {
        let mut parts = line.split(' ');

        let source = parts
            .next()
            .ok_or_else(|| Error::new(ErrorKind::Other, "Missing source"))?;
        let dest = parts
            .next()
            .ok_or_else(|| Error::new(ErrorKind::Other, "Missing dest"))?;

        Ok(MountInfo {
            source: PathBuf::from(Self::parse_value(source)?),
            dest:   PathBuf::from(Self::parse_value(dest)?),
        })
    }

    pub(crate) fn new() -> Result<Mounts> {
        let mut ret = Vec::new();

        let file = BufReader::new(File::open("/proc/mounts")?);
        for line_res in file.lines() {
            let line = line_res?;
            ret.push(Self::parse_line(&line)?);
        }

        Ok(Mounts(ret))
    }

    pub(crate) fn get_mount_point(&self, path: &Path) -> Option<PathBuf> {
        self.0
            .iter()
            .find(|mount| mount.source == path)
            .map(|mount| mount.dest.clone())
    }

    pub(crate) fn starts_with(&self, path: &[u8]) -> Vec<PathBuf> {
        self.0
            .iter()
            .filter(|mount| mount.source.as_os_str().len() >= path.len())
            .filter(|mount| &mount.source.as_os_str().as_bytes()[..path.len()] == path)
            .map(|mount| mount.dest.clone())
            .collect::<Vec<_>>()
    }

    pub(crate) fn mount_starts_with(&self, path: &[u8]) -> Vec<PathBuf> {
        self.0
            .iter()
            .filter(|mount| mount.dest.as_os_str().len() >= path.len())
            .filter(|mount| &mount.dest.as_os_str().as_bytes()[..path.len()] == path)
            .map(|mount| mount.dest.clone())
            .collect::<Vec<_>>()
    }
}
