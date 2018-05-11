extern crate distinst;
extern crate libc;
#[macro_use]
extern crate log;

use std::ffi::{CStr, CString};
use std::ptr;

pub use self::auto::*;
pub use self::config::*;
pub use self::disk::*;
pub use self::filesystem::*;
pub use self::installer::*;
pub use self::keyboard_layout::*;
pub use self::locale::*;
pub use self::lvm::*;
pub use self::partition::*;
pub use self::sector::*;

pub const DISTINST_MODIFY_BOOT_ORDER: u8 = 0b01;
pub const DISTINST_INSTALL_HARDWARE_SUPPORT: u8 = 0b10;

use std::io;

mod auto;
mod config;
mod disk;
mod ffi;
mod filesystem;
mod installer;
mod keyboard_layout;
mod locale;
mod lvm;
mod partition;
mod sector;

/// In comes a stack-allocated struct, and out goes a heap-allocated object.
pub fn gen_object_ptr<T>(obj: T) -> *mut T { Box::into_raw(Box::new(obj)) as *mut T }

pub fn null_check<T>(ptr: *const T, msg: &str) -> io::Result<*const T> {
    if ptr.is_null() {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: null pointer", msg),
        ))
    } else {
        Ok(ptr)
    }
}

pub fn get_str<'a>(ptr: *const libc::c_char, msg: &str) -> io::Result<&'a str> {
    null_check(ptr, msg).and_then(|ptr| {
        unsafe { CStr::from_ptr(ptr) }.to_str().map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: invalid UTF-8: {}", msg, err),
            )
        })
    })
}

pub fn to_cstr(string: String) -> *mut libc::c_char {
    CString::new(string)
        .map(|string| string.into_raw())
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_generate_unique_id(
    prefix: *const libc::c_char,
) -> *mut libc::c_char {
    get_str(prefix, "")
        .ok()
        .and_then(|prefix| distinst::generate_unique_id(prefix).ok().map(to_cstr))
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_validate_hostname(hostname: *const libc::c_char) -> bool {
    get_str(hostname, "")
        .ok()
        .map_or(false, |hostname| distinst::hostname::is_valid(hostname))
}

#[no_mangle]
pub extern "C" fn distinst_minimum_disk_size(size: u64) -> u64 { distinst::minimum_disk_size(size) }

/// Log level
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum DISTINST_LOG_LEVEL {
    TRACE,
    DEBUG,
    INFO,
    WARN,
    ERROR,
}

/// Installer log callback
pub type DistinstLogCallback = extern "C" fn(
    level: DISTINST_LOG_LEVEL,
    message: *const libc::c_char,
    user_data: *mut libc::c_void,
);

/// Initialize logging
#[no_mangle]
pub unsafe extern "C" fn distinst_log(
    callback: DistinstLogCallback,
    user_data: *mut libc::c_void,
) -> libc::c_int {
    use log::LogLevel;
    use DISTINST_LOG_LEVEL::*;

    let user_data_sync = user_data as usize;
    match distinst::log(move |level, message| {
        let c_level = match level {
            LogLevel::Trace => TRACE,
            LogLevel::Debug => DEBUG,
            LogLevel::Info => INFO,
            LogLevel::Warn => WARN,
            LogLevel::Error => ERROR,
        };
        let c_message = CString::new(message).unwrap();
        callback(
            c_level,
            c_message.as_ptr(),
            user_data_sync as *mut libc::c_void,
        );
    }) {
        Ok(()) => 0,
        Err(_err) => libc::EINVAL,
    }
}
