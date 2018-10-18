use distinst::{Config, timezones::Region};
use get_str;
use libc;
use std::io;
use timezones::DistinstRegion;

/// Installer configuration
#[repr(C)]
#[derive(Debug)]
pub struct DistinstConfig {
    hostname:         *const libc::c_char,
    keyboard_layout:  *const libc::c_char,
    keyboard_model:   *const libc::c_char,
    keyboard_variant: *const libc::c_char,
    old_root:         *const libc::c_char,
    lang:             *const libc::c_char,
    remove:           *const libc::c_char,
    squashfs:         *const libc::c_char,
    username:         *const libc::c_char,
    password:         *const libc::c_char,
    timezone:         *const DistinstRegion,
    flags:            u8,
}

impl DistinstConfig {
    pub unsafe fn as_config<'a>(&self) -> Result<Config<'a>, io::Error> {
        Ok(Config {
            squashfs:         get_str(self.squashfs)?.to_string(),
            hostname:         get_str(self.hostname)?.to_string(),
            lang:             get_str(self.lang)?.to_string(),
            keyboard_layout:  get_str(self.keyboard_layout)?.to_string(),
            keyboard_model:   get_str(self.keyboard_model).ok().map(String::from),
            keyboard_variant: get_str(self.keyboard_variant).ok().map(String::from),
            old_root:         get_str(self.old_root).ok().map(String::from),
            remove:           get_str(self.remove)?.to_string(),
            username:         get_str(self.username).ok().map(String::from),
            password:         get_str(self.password).ok().map(String::from),
            timezone:         if self.timezone.is_null() {
                None
            } else {
                Some(&*(self.timezone as *const Region))
            },
            flags:            self.flags,
        })
    }
}
