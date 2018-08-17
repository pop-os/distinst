use std::ffi::OsString;
use std::io::{Error, ErrorKind, Read, Result};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use misc::{self, watch_and_set};

lazy_static! {
    pub(crate) static ref SWAPS: Arc<RwLock<Swaps>> = {
        let swaps = Arc::new(RwLock::new(Swaps::new().unwrap()));
        watch_and_set(swaps.clone(), "/proc/swaps", || Swaps::new().ok());
        swaps
    };
}

#[derive(Debug, PartialEq)]
pub struct SwapInfo {
    pub source:   PathBuf,
    pub kind:     OsString,
    pub size:     OsString,
    pub used:     OsString,
    pub priority: OsString,
}

#[derive(Debug, PartialEq)]
pub struct Swaps(Vec<SwapInfo>);

impl Swaps {
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

    fn parse_line(line: &str) -> Result<SwapInfo> {
        let mut parts = line.split_whitespace();

        macro_rules! next_value {
            ($err:expr) => {{
                parts.next()
                    .ok_or_else(|| Error::new(ErrorKind::Other, $err))
                    .and_then(|val| Self::parse_value(val))
            }}
        }

        Ok(SwapInfo {
            source:   PathBuf::from(next_value!("Missing source")?),
            kind:     next_value!("Missing kind")?,
            size:     next_value!("Missing size")?,
            used:     next_value!("Missing used")?,
            priority: next_value!("Missing priority")?,
        })
    }

    pub fn parse_from<'a, I: Iterator<Item = &'a str>>(lines: I) -> Result<Swaps> {
        lines.map(Self::parse_line)
            .collect::<Result<Vec<SwapInfo>>>()
            .map(Swaps)
    }

    pub fn new() -> Result<Swaps> {
        let file = misc::open("/proc/swaps")
            .and_then(|mut file| {
                let length = file.metadata().ok().map_or(0, |x| x.len() as usize);
                let mut string = String::with_capacity(length);
                file.read_to_string(&mut string).map(|_| string)
            })?;

        Self::parse_from(file.lines().skip(1))
    }

    pub fn get_swapped(&self, path: &Path) -> bool {
        self.0.iter().any(|mount| mount.source == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::ffi::OsString;

    const SAMPLE: &str = r#"Filename				Type		Size	Used	Priority
/dev/sda5                               partition	8388600	0	-2"#;

    #[test]
    fn swaps() {
        let swaps = Swaps::parse_from(SAMPLE.lines().skip(1)).unwrap();
        assert_eq!(
            swaps,
            Swaps(vec![
                SwapInfo {
                    source: PathBuf::from("/dev/sda5"),
                    kind: OsString::from("partition"),
                    size: OsString::from("8388600"),
                    used: OsString::from("0"),
                    priority: OsString::from("-2")
                }
            ])
        );

        assert!(swaps.get_swapped(Path::new("/dev/sda5")));
        assert!(!swaps.get_swapped(Path::new("/dev/sda1")));
    }
}
