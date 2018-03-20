use libc;

use std::io;

use distinst::Config;

use get_str;

/// Installer configuration
#[repr(C)]
#[derive(Debug)]
pub struct DistinstConfig {
    hostname:         *const libc::c_char,
    keyboard_layout:  *const libc::c_char,
    keyboard_model:   *const libc::c_char,
    keyboard_variant: *const libc::c_char,
    lang:             *const libc::c_char,
    remove:           *const libc::c_char,
    squashfs:         *const libc::c_char,
}

impl DistinstConfig {
    pub unsafe fn as_config(&self) -> Result<Config, io::Error> {
        Ok(Config {
            squashfs:         get_str(self.squashfs, "config.squashfs")?.to_string(),
            hostname:         get_str(self.hostname, "config.hostname")?.to_string(),
            lang:             get_str(self.lang, "config.lang")?.to_string(),
            keyboard_layout:  get_str(self.keyboard_layout, "config.keyboard_layout")?.to_string(),
            keyboard_model:   get_str(self.keyboard_model, "").ok().map(String::from),
            keyboard_variant: get_str(self.keyboard_variant, "").ok().map(String::from),
            remove:           get_str(self.remove, "config.remove")?.to_string(),
        })
    }
}
