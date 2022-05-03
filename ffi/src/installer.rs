use libc;

use std::{io, mem};

use crate::config::DistinstConfig;
use crate::disk::DistinstDisks;
use distinst::{timezones::Region, Disks, Error, Installer, Status, Step};
use crate::gen_object_ptr;
use crate::DistinstRegion;
use crate::DistinstUserAccountCreate;

/// Bootloader steps
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum DISTINST_STEP {
    BACKUP,
    INIT,
    PARTITION,
    EXTRACT,
    CONFIGURE,
    BOOTLOADER,
}

impl From<DISTINST_STEP> for Step {
    fn from(step: DISTINST_STEP) -> Self {
        use DISTINST_STEP::*;
        match step {
            BACKUP => Step::Backup,
            INIT => Step::Init,
            PARTITION => Step::Partition,
            EXTRACT => Step::Extract,
            CONFIGURE => Step::Configure,
            BOOTLOADER => Step::Bootloader,
        }
    }
}

impl From<Step> for DISTINST_STEP {
    fn from(step: Step) -> Self {
        use DISTINST_STEP::*;
        match step {
            Step::Backup => BACKUP,
            Step::Init => INIT,
            Step::Partition => PARTITION,
            Step::Extract => EXTRACT,
            Step::Configure => CONFIGURE,
            Step::Bootloader => BOOTLOADER,
        }
    }
}

/// Installer error message
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DistinstError {
    step: DISTINST_STEP,
    err:  libc::c_int,
}

/// Installer error callback
pub type DistinstErrorCallback =
    extern "C" fn(status: *const DistinstError, user_data: *mut libc::c_void);

/// Installer status message
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DistinstStatus {
    step:    DISTINST_STEP,
    percent: libc::c_int,
}

/// Installer status callback
pub type DistinstStatusCallback =
    extern "C" fn(status: *const DistinstStatus, user_data: *mut libc::c_void);

/// Installer timezone callback
pub type DistinstTimezoneCallback =
    extern "C" fn(user_data: *mut libc::c_void) -> *const DistinstRegion;

/// Installer user account creation callback
pub type DistinstUserAccountCallback =
    extern "C" fn(user_account_create: *mut DistinstUserAccountCreate, user_data: *mut libc::c_void);

/// An installer object
#[repr(C)]
pub struct DistinstInstaller;

/// Create an installer object
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_new() -> *mut DistinstInstaller {
    gen_object_ptr(Installer::default()) as *mut DistinstInstaller
}

/// Send an installer status message
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_emit_error(
    installer: *mut DistinstInstaller,
    error: *const DistinstError,
) {
    (*(installer as *mut Installer)).emit_error(&Error {
        step: (*error).step.into(),
        err:  io::Error::from_raw_os_error((*error).err),
    });
}

/// Set the installer status callback
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_on_error(
    installer: *mut DistinstInstaller,
    callback: DistinstErrorCallback,
    user_data: *mut libc::c_void,
) {
    (*(installer as *mut Installer)).on_error(move |error| {
        callback(
            &DistinstError {
                step: error.step.into(),
                err:  error.err.raw_os_error().unwrap_or(libc::EIO),
            } as *const DistinstError,
            user_data,
        )
    });
}

/// Send an installer status message
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_emit_status(
    installer: *mut DistinstInstaller,
    status: *const DistinstStatus,
) {
    (*(installer as *mut Installer))
        .emit_status(Status { step: (*status).step.into(), percent: (*status).percent });
}

/// Set the installer status callback
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_on_status(
    installer: *mut DistinstInstaller,
    callback: DistinstStatusCallback,
    user_data: *mut libc::c_void,
) {
    (*(installer as *mut Installer)).on_status(move |status| {
        callback(
            &DistinstStatus { step: status.step.into(), percent: status.percent }
                as *const DistinstStatus,
            user_data,
        )
    });
}

#[no_mangle]
pub unsafe extern "C" fn distinst_installer_set_timezone_callback(
    installer: *mut DistinstInstaller,
    callback: DistinstTimezoneCallback,
    user_data: *mut libc::c_void,
) {
    (*(installer as *mut Installer))
        .set_timezone_callback(move || (&*(callback(user_data) as *const Region)).clone());
}

#[no_mangle]
pub unsafe extern "C" fn distinst_installer_set_user_callback(
    installer: *mut DistinstInstaller,
    callback: DistinstUserAccountCallback,
    user_data: *mut libc::c_void,
) {
    (*(installer as *mut Installer)).set_user_callback(move || {
        let mut user_account_create = mem::zeroed();
        callback(&mut user_account_create, user_data);
        user_account_create.as_config().expect("user callback invalid")
    });
}

/// Install using this installer, whilst retaining home & user accounts.
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_install(
    installer: *mut DistinstInstaller,
    disks: *mut DistinstDisks,
    config: *const DistinstConfig,
) -> libc::c_int {
    let disks: Box<Disks> = if disks.is_null() || installer.is_null() || config.is_null() {
        return libc::EIO;
    } else {
        Box::from_raw(disks as *mut Disks)
    };

    match (*config).as_config() {
        Ok(config) => match (*(installer as *mut Installer)).install(*disks, &config) {
            Ok(()) => 0,
            Err(err) => {
                info!("Install error: {}", err);
                err.raw_os_error().unwrap_or(libc::EIO)
            }
        },
        Err(err) => {
            info!("Config error: {}", err);
            let errno = err.raw_os_error().unwrap_or(libc::EIO);
            (*(installer as *mut Installer)).emit_error(&Error { step: Step::Init, err });
            errno
        }
    }
}

/// Destroy an installer object
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_destroy(installer: *mut DistinstInstaller) {
    if installer.is_null() {
        error!("DistinstInstaller was to be destroyed even though it is null");
    } else {
        Box::from_raw(installer as *mut Installer);
    }
}
