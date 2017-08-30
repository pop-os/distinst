extern crate libc;

use std::ffi::CStr;
use std::io;

use super::{Bootloader, Error, Installer, Status, Step};

/// Bootloader type
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum DISTINST_BOOTLOADER {
    BIOS,
    EFI,
}

impl From<DISTINST_BOOTLOADER> for Bootloader {
    fn from(step: DISTINST_BOOTLOADER) -> Self {
        use DISTINST_BOOTLOADER::*;
        match step{
            BIOS => Bootloader::Bios,
            EFI => Bootloader::Efi,
        }
    }
}

impl From<Bootloader> for DISTINST_BOOTLOADER {
    fn from(step: Bootloader) -> Self {
        use DISTINST_BOOTLOADER::*;
        match step{
            Bootloader::Bios => BIOS,
            Bootloader::Efi => EFI,
        }
    }
}

/// Bootloader steps
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum DISTINST_STEP {
    PARTITION,
    FORMAT,
    EXTRACT,
    BOOTLOADER,
}

impl From<DISTINST_STEP> for Step {
    fn from(step: DISTINST_STEP) -> Self {
        use DISTINST_STEP::*;
        match step{
            PARTITION => Step::Partition,
            FORMAT => Step::Format,
            EXTRACT => Step::Extract,
            BOOTLOADER => Step::Bootloader,
        }
    }
}

impl From<Step> for DISTINST_STEP {
    fn from(step: Step) -> Self {
        use DISTINST_STEP::*;
        match step{
            Step::Partition => PARTITION,
            Step::Format => FORMAT,
            Step::Extract => EXTRACT,
            Step::Bootloader => BOOTLOADER,
        }
    }
}

/// An installer object
#[repr(C)]
pub struct DistinstInstaller(Installer);

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

/// Create an installer object
#[no_mangle]
pub extern fn distinst_installer_new() -> *mut DistinstInstaller {
    Box::into_raw(Box::new(DistinstInstaller(Installer::new())))
}

/// Send an installer status message
#[no_mangle]
pub unsafe extern fn distinst_installer_emit_error(installer: *mut DistinstInstaller, error: *const DistinstError) {
    (*installer).0.emit_error(
        &Error {
            step: (*error).step.into(),
            err: io::Error::from_raw_os_error((*error).err)
        }
    );
}

/// Set the installer status callback
#[no_mangle]
pub unsafe extern fn distinst_installer_on_error(installer: *mut DistinstInstaller, callback: DistinstErrorCallback, user_data: * mut libc::c_void)
{
    (*installer).0.on_error(move |error| {
        callback(
            & DistinstError {
                step: error.step.into(),
                err: error.err.raw_os_error().unwrap_or(0),
            } as *const DistinstError,
            user_data
        )
    });
}

/// Send an installer status message
#[no_mangle]
pub unsafe extern fn distinst_installer_emit_status(installer: *mut DistinstInstaller, status: *const DistinstStatus) {
    (*installer).0.emit_status(
        &Status {
            step: (*status).step.into(),
            percent: (*status).percent
        }
    );
}

/// Set the installer status callback
#[no_mangle]
pub unsafe extern fn distinst_installer_on_status(installer: *mut DistinstInstaller, callback: DistinstStatusCallback, user_data: * mut libc::c_void) {
    (*installer).0.on_status(move |status| {
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
pub unsafe extern fn distinst_installer_install(installer: *mut DistinstInstaller, drive: *const libc::c_char, bootloader: DISTINST_BOOTLOADER) {
    let cstr = CStr::from_ptr(drive);
    match cstr.to_str() {
        Ok(string) => {
            (*installer).0.install(
                string,
                match bootloader {
                    DISTINST_BOOTLOADER::BIOS => Bootloader::Bios,
                    DISTINST_BOOTLOADER::EFI => Bootloader::Efi,
                }
            );
        },
        Err(err) => {
            println!("install error: {}", err);
        }
    }
}

/// Destroy an installer object
#[no_mangle]
pub unsafe extern fn distinst_installer_destroy(installer: *mut DistinstInstaller) {
    drop(Box::from_raw(installer))
}
