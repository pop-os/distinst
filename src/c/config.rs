use libc;

use std::ffi::CStr;
use std::io;

use Config;

/// Installer configuration
#[repr(C)]
#[derive(Debug)]
pub struct DistinstConfig {
    squashfs: *const libc::c_char,
    lang: *const libc::c_char,
    remove: *const libc::c_char,
}

impl DistinstConfig {
    pub unsafe fn into_config(&self) -> Result<Config, io::Error> {
        if self.squashfs.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "config.squashfs: null pointer",
            ));
        }

        let squashfs = CStr::from_ptr(self.squashfs).to_str().map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("config.squashfs: invalid UTF-8: {}", err),
            )
        })?;

        if self.lang.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "config.lang: null pointer",
            ));
        }

        let lang = CStr::from_ptr(self.lang).to_str().map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("config.lang: invalid UTF-8: {}", err),
            )
        })?;

        if self.remove.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "config.remove: null pointer",
            ));
        }

        let remove = CStr::from_ptr(self.remove).to_str().map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("config.remove: invalid UTF-8: {}", err),
            )
        })?;

        Ok(Config {
            squashfs: squashfs.to_string(),
            lang: lang.to_string(),
            remove: remove.to_string(),
        })
    }
}
