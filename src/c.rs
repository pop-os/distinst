extern crate libc;

use std::ffi::{CStr, CString};
use std::io;

use super::{log, Config, Error, Installer, Status, Step};

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
pub type DistinstLogCallback = extern "C" fn(level: DISTINST_LOG_LEVEL, message: *const libc::c_char, user_data: * mut libc::c_void);

/// Bootloader steps
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum DISTINST_STEP {
    INIT,
    PARTITION,
    FORMAT,
    EXTRACT,
    CONFIGURE,
    BOOTLOADER,
}

impl From<DISTINST_STEP> for Step {
    fn from(step: DISTINST_STEP) -> Self {
        use DISTINST_STEP::*;
        match step{
            INIT => Step::Init,
            PARTITION => Step::Partition,
            FORMAT => Step::Format,
            EXTRACT => Step::Extract,
            CONFIGURE => Step::Configure,
            BOOTLOADER => Step::Bootloader,
        }
    }
}

impl From<Step> for DISTINST_STEP {
    fn from(step: Step) -> Self {
        use DISTINST_STEP::*;
        match step{
            Step::Init => INIT,
            Step::Partition => PARTITION,
            Step::Format => FORMAT,
            Step::Extract => EXTRACT,
            Step::Configure => CONFIGURE,
            Step::Bootloader => BOOTLOADER,
        }
    }
}

/// Installer configuration
#[repr(C)]
#[derive(Debug)]
pub struct DistinstConfig {
    squashfs: *const libc::c_char,
    disk: *const libc::c_char,
    lang: *const libc::c_char,
}

impl DistinstConfig {
    unsafe fn into_config(&self) -> Result<Config, io::Error> {
        let squashfs_cstr = CStr::from_ptr(self.squashfs);
        let squashfs = squashfs_cstr.to_str().map_err(|err| {
            io::Error::new(io::ErrorKind::InvalidData, format!("config.squashfs: Invalid UTF-8: {}", err))
        })?;

        let disk_cstr = CStr::from_ptr(self.disk);
        let disk = disk_cstr.to_str().map_err(|err| {
            io::Error::new(io::ErrorKind::InvalidData, format!("config.disk: Invalid UTF-8: {}", err))
        })?;

        let lang_cstr = CStr::from_ptr(self.lang);
        let lang = lang_cstr.to_str().map_err(|err| {
            io::Error::new(io::ErrorKind::InvalidData, format!("config.lang: Invalid UTF-8: {}", err))
        })?;

        Ok(Config {
            squashfs: squashfs.to_string(),
            disk: disk.to_string(),
            lang: lang.to_string(),
        })
    }
}

/// Installer error message
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DistinstError {
    step: DISTINST_STEP,
    err: libc::c_int,
}

/// Installer error callback
pub type DistinstErrorCallback = extern "C" fn(status: *const DistinstError, user_data: * mut libc::c_void);

/// Installer status message
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DistinstStatus {
    step: DISTINST_STEP,
    percent: libc::c_int,
}

/// Installer status callback
pub type DistinstStatusCallback = extern "C" fn(status: *const DistinstStatus, user_data: * mut libc::c_void);

/// An installer object
#[repr(C)]
pub struct DistinstInstaller;

/// Initialize logging
#[no_mangle]
pub unsafe extern "C" fn distinst_log(callback: DistinstLogCallback, user_data: * mut libc::c_void) -> libc::c_int {
    use DISTINST_LOG_LEVEL::*;
    use log::LogLevel;

    let user_data_sync = user_data as usize;
    match log(move |level, message| {
        let c_level = match level {
            LogLevel::Trace => TRACE,
            LogLevel::Debug => DEBUG,
            LogLevel::Info => INFO,
            LogLevel::Warn => WARN,
            LogLevel::Error => ERROR,
        };
        let c_message = CString::new(message).unwrap();
        callback(c_level, c_message.as_ptr(), user_data_sync as * mut libc::c_void);
    }) {
        Ok(()) => 0,
        Err(_err) => libc::EINVAL
    }
}

/// Create an installer object
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_new() -> *mut DistinstInstaller {
    Box::into_raw(Box::new(Installer::new())) as *mut DistinstInstaller
}

/// Send an installer status message
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_emit_error(installer: *mut DistinstInstaller, error: *const DistinstError) {
    (*(installer as *mut Installer)).emit_error(
        &Error {
            step: (*error).step.into(),
            err: io::Error::from_raw_os_error((*error).err)
        }
    );
}

/// Set the installer status callback
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_on_error(installer: *mut DistinstInstaller, callback: DistinstErrorCallback, user_data: * mut libc::c_void)
{
    (*(installer as *mut Installer)).on_error(move |error| {
        callback(
            & DistinstError {
                step: error.step.into(),
                err: error.err.raw_os_error().unwrap_or(libc::EIO),
            } as *const DistinstError,
            user_data
        )
    });
}

/// Send an installer status message
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_emit_status(installer: *mut DistinstInstaller, status: *const DistinstStatus) {
    (*(installer as *mut Installer)).emit_status(
        &Status {
            step: (*status).step.into(),
            percent: (*status).percent
        }
    );
}

/// Set the installer status callback
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_on_status(installer: *mut DistinstInstaller, callback: DistinstStatusCallback, user_data: * mut libc::c_void) {
    (*(installer as *mut Installer)).on_status(move |status| {
        callback(
            &DistinstStatus {
                step: status.step.into(),
                percent: status.percent,
            } as *const DistinstStatus,
            user_data
        )
    });
}

/// Install using this installer
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_install(installer: *mut DistinstInstaller, config: *const DistinstConfig) -> libc::c_int {
    match (*config).into_config() {
        Ok(config) => {
            match (*(installer as *mut Installer)).install(&config) {
                Ok(()) => 0,
                Err(err) => {
                    info!("Install error: {}", err);
                    err.raw_os_error().unwrap_or(libc::EIO)
                }
            }
        },
        Err(err) => {
            info!("Config error: {}", err);
            let errno = err.raw_os_error().unwrap_or(libc::EIO);
            (*(installer as *mut Installer)).emit_error(&Error {
                step: Step::Init,
                err: err,
            });
            errno
        }
    }
}

/// Destroy an installer object
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_destroy(installer: *mut DistinstInstaller) {
    drop(Box::from_raw(installer as *mut Installer))
}
