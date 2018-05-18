use libc;

use super::{gen_object_ptr, get_str, DistinstDisks};
use distinst::auto::{EraseOption, InstallOption, InstallOptions, RecoveryOption, RefreshOption};
use distinst::Disks;
use std::os::unix::ffi::OsStrExt;
use std::ptr;

#[repr(C)]
pub struct DistinstRefreshOption;

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_os_name(
    option: *const DistinstRefreshOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RefreshOption);
    let output = option.os_name.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_os_version(
    option: *const DistinstRefreshOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RefreshOption);
    let output = option.os_version.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_root_part(
    option: *const DistinstRefreshOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RefreshOption);
    let output = option.root_part.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[repr(C)]
pub struct DistinstEraseOption;

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_get_device_path(
    option: *const DistinstEraseOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const EraseOption);
    let output = option.device.as_os_str().as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_get_model(
    option: *const DistinstEraseOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const EraseOption);
    let output = option.model.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_get_linux_icon(
    option: *const DistinstEraseOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const EraseOption);
    let output = option.get_linux_icon().as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_get_sectors(
    option: *const DistinstEraseOption,
) -> libc::uint64_t {
    let option = &*(option as *const EraseOption);
    option.sectors
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_is_rotational(
    option: *const DistinstEraseOption,
) -> bool {
    let option = &*(option as *const EraseOption);
    option.is_rotational()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_is_removable(
    option: *const DistinstEraseOption,
) -> bool {
    let option = &*(option as *const EraseOption);
    option.is_removable()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_meets_requirements(
    option: *const DistinstEraseOption,
) -> bool {
    let option = &*(option as *const EraseOption);
    option.meets_requirements()
}

#[repr(C)]
pub struct DistinstRecoveryOption;

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_efi_uuid(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RecoveryOption);
    match option.efi_uuid.as_ref() {
        Some(ref efi_uuid) => {
            let output = efi_uuid.as_bytes();
            *len = output.len() as libc::c_int;
            output.as_ptr()
        }
        None => ptr::null(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_hostname(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RecoveryOption);
    let output = option.hostname.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_kbd_layout(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RecoveryOption);
    let output = option.kbd_layout.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_language(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RecoveryOption);
    let output = option.language.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_recovery_uuid(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RecoveryOption);
    let output = option.recovery_uuid.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_root_uuid(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RecoveryOption);
    let output = option.root_uuid.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_kbd_model(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RecoveryOption);
    match option.kbd_model.as_ref() {
        Some(ref kbd_model) => {
            let output = kbd_model.as_bytes();
            *len = output.len() as libc::c_int;
            output.as_ptr()
        }
        None => ptr::null(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_kbd_variant(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const RecoveryOption);
    match option.kbd_variant.as_ref() {
        Some(ref kbd_variant) => {
            let output = kbd_variant.as_bytes();
            *len = output.len() as libc::c_int;
            output.as_ptr()
        }
        None => ptr::null(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_oem_mode(
    option: *const DistinstRecoveryOption,
) -> bool {
    let option = &*(option as *const RecoveryOption);
    option.oem_mode
}

#[repr(C)]
pub enum DISTINST_INSTALL_OPTION_VARIANT {
    REFRESH,
    ERASE,
    RECOVERY,
}

#[repr(C)]
pub struct DistinstInstallOption {
    tag:          DISTINST_INSTALL_OPTION_VARIANT,
    option:       *const libc::c_void,
    encrypt_pass: *const libc::c_char,
}

impl<'a> From<&'a DistinstInstallOption> for InstallOption<'a> {
    fn from(opt: &'a DistinstInstallOption) -> InstallOption<'a> {
        let get_passwd = || {
            if opt.encrypt_pass.is_null() {
                None
            } else {
                get_str(opt.encrypt_pass, "").ok().map(String::from)
            }
        };

        unsafe {
            match opt.tag {
                DISTINST_INSTALL_OPTION_VARIANT::RECOVERY => InstallOption::RecoveryOption {
                    option:   &*(opt.option as *const RecoveryOption),
                    password: get_passwd(),
                },
                DISTINST_INSTALL_OPTION_VARIANT::REFRESH => {
                    InstallOption::RefreshOption(&*(opt.option as *const RefreshOption))
                }
                DISTINST_INSTALL_OPTION_VARIANT::ERASE => InstallOption::EraseOption {
                    option:   &*(opt.option as *const EraseOption),
                    password: get_passwd(),
                },
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_option_new() -> *mut DistinstInstallOption {
    Box::into_raw(Box::new(DistinstInstallOption {
        tag:          DISTINST_INSTALL_OPTION_VARIANT::ERASE,
        option:       ptr::null(),
        encrypt_pass: ptr::null(),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_option_destroy(option: *mut DistinstInstallOption) {
    Box::from_raw(option);
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_option_apply(
    option: *const DistinstInstallOption,
    disks: *mut DistinstDisks,
) -> libc::c_int {
    match InstallOption::from(&*option).apply(&mut *(disks as *mut Disks)) {
        Ok(()) => 0,
        Err(why) => {
            warn!("failed to apply install option: {}", why);
            -1
        }
    }
}

#[repr(C)]
pub struct DistinstInstallOptions;

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_new(
    disks: *const DistinstDisks,
    required: libc::uint64_t,
) -> *mut DistinstInstallOptions {
    if disks.is_null() {
        ptr::null_mut()
    } else {
        let options = InstallOptions::new(&*(disks as *const Disks), required);
        gen_object_ptr(options) as *mut DistinstInstallOptions
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_destroy(options: *mut DistinstInstallOptions) {
    if !options.is_null() {
        drop(Box::from_raw(options as *mut InstallOptions))
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_get_refresh_options(
    options: *const DistinstInstallOptions,
    len: *mut libc::c_int,
) -> *mut *const DistinstRefreshOption {
    let options = &*(options as *const InstallOptions);

    let mut output: Vec<*const DistinstRefreshOption> = Vec::new();
    for option in options.refresh_options.iter() {
        output.push(option as *const RefreshOption as *const DistinstRefreshOption);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *const DistinstRefreshOption
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_get_erase_options(
    options: *const DistinstInstallOptions,
    len: *mut libc::c_int,
) -> *mut *const DistinstEraseOption {
    let options = &*(options as *const InstallOptions);

    let mut output: Vec<*const DistinstEraseOption> = Vec::new();
    for option in options.erase_options.iter() {
        output.push(option as *const EraseOption as *const DistinstEraseOption);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *const DistinstEraseOption
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_get_recovery_option(
    options: *const DistinstInstallOptions,
) -> *const DistinstRecoveryOption {
    let options = &*(options as *const InstallOptions);
    options
        .recovery_option
        .as_ref()
        .map(|opt| opt as *const RecoveryOption as *const DistinstRecoveryOption)
        .unwrap_or(ptr::null())
}
