use libc;

use super::{gen_object_ptr, get_str, null_check, DistinstDisks, DistinstOsRelease};
use distinst::{
    auto::{
        AlongsideMethod, AlongsideOption, EraseOption, InstallOption, InstallOptions,
        RecoveryOption, RefreshOption,
    },
    Disks, OS,
};
use std::{os::unix::ffi::OsStrExt, ptr};

#[repr(C)]
pub struct DistinstAlongsideOption;

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_get_device(
    option: *const DistinstAlongsideOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const AlongsideOption);
    let output = option.device.as_os_str().as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_get_os(
    option: *const DistinstAlongsideOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const AlongsideOption);
    let output = option.get_os().as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_get_os_release(
    option: *const DistinstAlongsideOption,
    os_release: *mut DistinstOsRelease,
) -> libc::c_int {
    if option.is_null() {
        1
    } else {
        let option = &*(option as *const AlongsideOption);
        if let Some(OS::Linux { ref info, .. }) = option.alongside {
            *os_release = DistinstOsRelease::from_os_release(info);
            0
        } else {
            2
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_is_linux(
    option: *const DistinstAlongsideOption,
) -> bool {
    let option = &*(option as *const AlongsideOption);
    if let Some(OS::Linux { .. }) = option.alongside {
        true
    } else {
        false
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_is_mac_os(
    option: *const DistinstAlongsideOption,
) -> bool {
    let option = &*(option as *const AlongsideOption);
    if let Some(OS::MacOs(_)) = option.alongside {
        true
    } else {
        false
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_is_windows(
    option: *const DistinstAlongsideOption,
) -> bool {
    let option = &*(option as *const AlongsideOption);
    if let Some(OS::Windows(_)) = option.alongside {
        true
    } else {
        false
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_get_partition(
    option: *const DistinstAlongsideOption,
) -> libc::c_int {
    let option = &*(option as *const AlongsideOption);
    match option.method {
        AlongsideMethod::Shrink { partition, .. } => partition,
        _ => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_get_path(
    option: *const DistinstAlongsideOption,
    len: *mut libc::c_int,
) -> *const u8 {
    let option = &*(option as *const AlongsideOption);
    match option.method {
        AlongsideMethod::Shrink { ref path, .. } => {
            let bytes = path.as_os_str().as_bytes();
            *len = bytes.len() as libc::c_int;
            bytes.as_ptr()
        }
        _ => ptr::null(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_get_sectors_free(
    option: *const DistinstAlongsideOption,
) -> u64 {
    let option = &*(option as *const AlongsideOption);
    match option.method {
        AlongsideMethod::Shrink { sectors_free, .. } => sectors_free,
        AlongsideMethod::Free(ref region) => region.size(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_alongside_option_get_sectors_total(
    option: *const DistinstAlongsideOption,
) -> u64 {
    let option = &*(option as *const AlongsideOption);
    match option.method {
        AlongsideMethod::Shrink { sectors_total, .. } => sectors_total,
        AlongsideMethod::Free(ref region) => region.size(),
    }
}

#[repr(C)]
pub struct DistinstRefreshOption;

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_can_retain_old(
    option: *const DistinstRefreshOption,
) -> bool {
    if null_check(option).is_err() {
        return false;
    }

    (&*(option as *const RefreshOption)).can_retain_old
}

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_os_release(
    option: *const DistinstRefreshOption,
    os_release: *mut DistinstOsRelease,
) -> libc::c_int {
    if option.is_null() {
        1
    } else {
        let option = &*(option as *const RefreshOption);
        *os_release = DistinstOsRelease::from_os_release(&option.os_release);
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_os_name(
    option: *const DistinstRefreshOption,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let option = &*(option as *const RefreshOption);
    let output = option.os_release.name.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_os_pretty_name(
    option: *const DistinstRefreshOption,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let option = &*(option as *const RefreshOption);
    let output = option.os_release.pretty_name.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_os_version(
    option: *const DistinstRefreshOption,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let option = &*(option as *const RefreshOption);
    let output = option.os_release.version.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_refresh_option_get_root_part(
    option: *const DistinstRefreshOption,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let option = &*(option as *const EraseOption);
    let output = option.get_linux_icon().as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_get_sectors(
    option: *const DistinstEraseOption,
) -> u64 {
    if null_check(option).is_err() {
        return 0;
    }

    let option = &*(option as *const EraseOption);
    option.sectors
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_is_rotational(
    option: *const DistinstEraseOption,
) -> bool {
    if null_check(option).is_err() {
        return false;
    }

    let option = &*(option as *const EraseOption);
    option.is_rotational()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_is_removable(
    option: *const DistinstEraseOption,
) -> bool {
    if null_check(option).is_err() {
        return false;
    }

    let option = &*(option as *const EraseOption);
    option.is_removable()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_erase_option_meets_requirements(
    option: *const DistinstEraseOption,
) -> bool {
    if null_check(option).is_err() {
        return false;
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let option = &*(option as *const RecoveryOption);
    let output = option.root_uuid.as_bytes();
    *len = output.len() as libc::c_int;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_luks_uuid(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let option = &*(option as *const RecoveryOption);
    match option.luks_uuid {
        Some(ref uuid) => {
            let output = uuid.as_bytes();
            *len = output.len() as libc::c_int;
            output.as_ptr()
        }
        None => {
            *len = 0;
            ptr::null()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_get_kbd_model(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

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
    if null_check(option).is_err() {
        return false;
    }

    let option = &*(option as *const RecoveryOption);
    option.oem_mode
}

#[no_mangle]
pub unsafe extern "C" fn distinst_recovery_option_mode(
    option: *const DistinstRecoveryOption,
    len: *mut libc::c_int,
) -> *const u8 {
    if option.is_null() {
        return ptr::null();
    }

    (&*(option as *const RecoveryOption)).mode.as_ref().map_or(ptr::null(), |mode| {
        *len = mode.len() as libc::c_int;
        mode.as_bytes().as_ptr()
    })
}

#[repr(C)]
pub enum DISTINST_INSTALL_OPTION_VARIANT {
    ALONGSIDE,
    ERASE,
    RECOVERY,
    REFRESH,
    UPGRADE,
}

#[repr(C)]
pub struct DistinstInstallOption {
    tag:          DISTINST_INSTALL_OPTION_VARIANT,
    option:       *const libc::c_void,
    encrypt_pass: *const libc::c_char,
    sectors:      u64,
}

impl<'a> From<&'a DistinstInstallOption> for InstallOption<'a> {
    fn from(opt: &'a DistinstInstallOption) -> InstallOption<'a> {
        let get_passwd = || {
            if opt.encrypt_pass.is_null() {
                None
            } else {
                get_str(opt.encrypt_pass).ok().map(String::from)
            }
        };

        unsafe {
            match opt.tag {
                DISTINST_INSTALL_OPTION_VARIANT::ALONGSIDE => InstallOption::Alongside {
                    option:   &*(opt.option as *const AlongsideOption),
                    password: get_passwd(),
                    sectors:  opt.sectors,
                },
                DISTINST_INSTALL_OPTION_VARIANT::RECOVERY => InstallOption::Recovery {
                    option:   &*(opt.option as *const RecoveryOption),
                    password: get_passwd(),
                },
                DISTINST_INSTALL_OPTION_VARIANT::REFRESH => {
                    InstallOption::Refresh(&*(opt.option as *const RefreshOption))
                }
                DISTINST_INSTALL_OPTION_VARIANT::ERASE => InstallOption::Erase {
                    option:   &*(opt.option as *const EraseOption),
                    password: get_passwd(),
                },
                DISTINST_INSTALL_OPTION_VARIANT::UPGRADE => {
                    InstallOption::Upgrade(&*(opt.option as *const RecoveryOption))
                }
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
        sectors:      0,
    }))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_option_destroy(option: *mut DistinstInstallOption) {
    if !option.is_null() {
        Box::from_raw(option);
    } else {
        error!("DistinstInstallOption was to be destroyed even though it is null");
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_option_apply(
    option: *const DistinstInstallOption,
    disks: *mut DistinstDisks,
) -> libc::c_int {
    if null_check(disks).or_else(|_| null_check(option)).is_err() {
        return libc::EIO;
    }

    match InstallOption::from(&*option).apply(&mut *(disks as *mut Disks)) {
        Ok(()) => 0,
        Err(why) => {
            error!("failed to apply install option: {}", why);
            -1
        }
    }
}

#[repr(C)]
pub struct DistinstInstallOptions;

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_new(
    disks: *const DistinstDisks,
    required: u64,
    shrink_overhead: u64,
) -> *mut DistinstInstallOptions {
    if null_check(disks).is_err() {
        return ptr::null_mut();
    }

    let options = InstallOptions::new(&*(disks as *const Disks), required, shrink_overhead);
    gen_object_ptr(options) as *mut DistinstInstallOptions
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_destroy(options: *mut DistinstInstallOptions) {
    if !options.is_null() {
        Box::from_raw(options as *mut InstallOptions);
    } else {
        error!("DistinstInstallOptions was to be destroyed even though it is null");
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_has_alongside_options(
    options: *const DistinstInstallOptions,
) -> bool {
    if null_check(options).is_err() {
        return false;
    }

    let options = &*(options as *const InstallOptions);
    !options.alongside_options.is_empty()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_get_alongside_options(
    options: *const DistinstInstallOptions,
    len: *mut libc::c_int,
) -> *mut *const DistinstAlongsideOption {
    if null_check(options).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let options = &*(options as *const InstallOptions);

    let mut output: Vec<*const DistinstAlongsideOption> = Vec::new();
    for option in &options.alongside_options {
        output.push(option as *const AlongsideOption as *const DistinstAlongsideOption);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *const DistinstAlongsideOption
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_has_refresh_options(
    options: *const DistinstInstallOptions,
) -> bool {
    if null_check(options).is_err() {
        return false;
    }

    let options = &*(options as *const InstallOptions);
    !options.refresh_options.is_empty()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_get_refresh_options(
    options: *const DistinstInstallOptions,
    len: *mut libc::c_int,
) -> *mut *const DistinstRefreshOption {
    if null_check(options).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let options = &*(options as *const InstallOptions);

    let mut output: Vec<*const DistinstRefreshOption> = Vec::new();
    for option in &options.refresh_options {
        output.push(option as *const RefreshOption as *const DistinstRefreshOption);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *const DistinstRefreshOption
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_has_erase_options(
    options: *const DistinstInstallOptions,
) -> bool {
    if null_check(options).is_err() {
        return false;
    }

    let options = &*(options as *const InstallOptions);
    !options.erase_options.is_empty()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_get_erase_options(
    options: *const DistinstInstallOptions,
    len: *mut libc::c_int,
) -> *mut *const DistinstEraseOption {
    if null_check(options).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let options = &*(options as *const InstallOptions);

    let mut output: Vec<*const DistinstEraseOption> = Vec::new();
    for option in &options.erase_options {
        output.push(option as *const EraseOption as *const DistinstEraseOption);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *const DistinstEraseOption
}

#[no_mangle]
pub unsafe extern "C" fn distinst_install_options_get_recovery_option(
    options: *const DistinstInstallOptions,
) -> *const DistinstRecoveryOption {
    if null_check(options).is_err() {
        return ptr::null();
    }

    let options = &*(options as *const InstallOptions);
    options
        .recovery_option
        .as_ref()
        .map(|opt| opt as *const RecoveryOption as *const DistinstRecoveryOption)
        .unwrap_or(ptr::null())
}
