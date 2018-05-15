use libc;

use std::ptr;
use std::os::unix::ffi::OsStrExt;
use super::{gen_object_ptr, get_str, DistinstDisks};
use distinst::Disks;
use distinst::auto::{RefreshOption, EraseOption, InstallOption, InstallOptions};

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
    option: *const DistinstEraseOption
) -> libc::uint64_t {
    let option = &*(option as *const EraseOption);
    option.sectors
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_is_rotational(
    option: *const DistinstEraseOption
) -> bool {
    let option = &*(option as *const EraseOption);
    option.is_rotational()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_is_removable(
    option: *const DistinstEraseOption
) -> bool {
    let option = &*(option as *const EraseOption);
    option.is_removable()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_meets_requirements(
    option: *const DistinstEraseOption
) -> bool {
    let option = &*(option as *const EraseOption);
    option.meets_requirements()
}

#[repr(C)]
pub enum DISTINST_INSTALL_OPTION_VARIANT {
    REFRESH,
    ERASE
}

#[repr(C)]
pub struct DistinstInstallOption {
    tag: DISTINST_INSTALL_OPTION_VARIANT,
    refresh_option: *const DistinstRefreshOption,
    erase_option: *const DistinstEraseOption,
    erase_pass: *const libc::c_char,
}

impl<'a> From<&'a DistinstInstallOption> for InstallOption<'a> {
    fn from(opt: &'a DistinstInstallOption) -> InstallOption<'a> {
        unsafe {
            match opt.tag {
                DISTINST_INSTALL_OPTION_VARIANT::REFRESH => {
                    InstallOption::RefreshOption(&*(opt.refresh_option as *const RefreshOption))
                }
                DISTINST_INSTALL_OPTION_VARIANT::ERASE => {
                    InstallOption::EraseOption {
                        option: &*(opt.erase_option as *const EraseOption),
                        password: if opt.erase_pass.is_null() {
                            None
                        } else {
                            get_str(opt.erase_pass, "").ok().map(String::from)
                        }
                    }
                }
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_option_new() -> *mut DistinstInstallOption {
    Box::into_raw(Box::new(DistinstInstallOption {
        tag: DISTINST_INSTALL_OPTION_VARIANT::ERASE,
        refresh_option: ptr::null(),
        erase_option: ptr::null(),
        erase_pass: ptr::null()
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
pub unsafe extern "C" fn distinst_install_options_destroy(
    options: *mut DistinstInstallOptions
) {
    if !options.is_null() {
        drop(Box::from_raw(options as *mut InstallOptions))
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_get_refresh_options(
    options: *const DistinstInstallOptions,
    len: *mut libc::c_int
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
    len: *mut libc::c_int
) -> *mut *const DistinstEraseOption {
    let options = &*(options as *const InstallOptions);

    let mut output: Vec<*const DistinstEraseOption> = Vec::new();
    for option in options.erase_options.iter() {
        output.push(option as *const EraseOption as *const DistinstEraseOption);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *const DistinstEraseOption
}
