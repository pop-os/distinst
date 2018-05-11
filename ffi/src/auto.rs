use libc;

use std::ptr;
use super::{gen_object_ptr, get_str, to_cstr, DistinstDisks};
use distinst::Disks;
use distinst::auto::{RefreshOption, EraseOption, InstallOption, InstallOptions};

#[repr(C)]
pub struct DistinstRefreshOption;

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_os_name(
    option: *const DistinstRefreshOption
) -> *mut libc::c_char {
    let option = &*(option as *const RefreshOption);
    to_cstr(option.os_name.clone())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_os_version(
    option: *const DistinstRefreshOption
) -> *mut libc::c_char {
    let option = &*(option as *const RefreshOption);
    to_cstr(option.os_version.clone())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_root_part(
    option: *const DistinstRefreshOption
) -> *mut libc::c_char {
    let option = &*(option as *const RefreshOption);
    to_cstr(option.root_part.clone())
}

#[repr(C)]
pub struct DistinstEraseOption;

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_get_device(
    option: *const DistinstEraseOption
) -> *mut libc::c_char {
    let option = &*(option as *const EraseOption);
    to_cstr(option.device.to_str().expect("device path is not UTF-8").into())
}

#[repr(C)]
pub enum DISTINST_INSTALL_OPTION_VARIANT {
    REFRESH,
    ERASE
}

#[repr(C)]
pub struct DistinstInstallOption {
    tag: DISTINST_INSTALL_OPTION_VARIANT,
    refresh_option: *mut DistinstRefreshOption,
    erase_option: *mut DistinstEraseOption,
    erase_pass: *const libc::c_char,
}

impl<'a> From<DistinstInstallOption> for InstallOption<'a> {
    fn from(opt: DistinstInstallOption) -> InstallOption<'a> {
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
