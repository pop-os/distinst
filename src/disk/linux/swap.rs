use std::char;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader, Error, ErrorKind, Result};
use std::os::unix::ffi::OsStringExt;

pub struct Swap {
    pub source: OsString,
    pub kind: OsString,
    pub size: OsString,
    pub used: OsString,
    pub priority: OsString,
}

impl Swap {
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
                            code += u32::from_str_radix(&(b as char).to_string(), 8).map_err(|err| {
                                Error::new(ErrorKind::Other, err)
                            })?;
                        } else {
                            return Err(Error::new(ErrorKind::Other, "truncated octal code"));
                        }
                    }
                    ret.push(code as u8);
                },
                _ => {
                    ret.push(b);
                }
            }
        }

        Ok(OsString::from_vec(ret))
    }

    fn parse_line(line: &str) -> Result<Swap> {
        let mut parts = line.split_whitespace();

        let source = parts.next().ok_or(Error::new(ErrorKind::Other, "Missing source"))?;
        let kind = parts.next().ok_or(Error::new(ErrorKind::Other, "Missing kind"))?;
        let size = parts.next().ok_or(Error::new(ErrorKind::Other, "Missing size"))?;
        let used = parts.next().ok_or(Error::new(ErrorKind::Other, "Missing used"))?;
        let priority = parts.next().ok_or(Error::new(ErrorKind::Other, "Missing priority"))?;

        Ok(Swap {
            source: Self::parse_value(&source)?,
            kind: Self::parse_value(&kind)?,
            size: Self::parse_value(&size)?,
            used: Self::parse_value(&used)?,
            priority: Self::parse_value(&priority)?
        })
    }

    pub fn all() -> Result<Vec<Swap>> {
        let mut ret = Vec::new();

        let file = BufReader::new(File::open("/proc/swaps")?);
        for line_res in file.lines().skip(1) {
            let line = line_res?;
            ret.push(Self::parse_line(&line)?);
        }

        Ok(ret)
    }
}
