use libc;

use super::get_str;
use std::io;

use Config;

/// Installer configuration
#[repr(C)]
#[derive(Debug)]
pub struct DistinstConfig {
    hostname: *const libc::c_char,
    keyboard: *const libc::c_char,
    lang:     *const libc::c_char,
    remove:   *const libc::c_char,
    squashfs: *const libc::c_char,
}

impl DistinstConfig {
    pub unsafe fn as_config(&self) -> Result<Config, io::Error> {
        Ok(Config {
            squashfs: get_str(self.squashfs, "config.squashfs")?.to_string(),
            hostname: get_str(self.hostname, "config.hostname")?.to_string(),
            lang:     get_str(self.lang, "config.lang")?.to_string(),
            keyboard: get_str(self.keyboard, "config.keyboard")?.to_string(),
            remove:   get_str(self.remove, "config.remove")?.to_string(),
        })
    }
}
