use super::null_check;
use distinst::os_release::{OsRelease, OS_RELEASE};
use libc;
use std::{ffi::CString, mem::forget, ptr};

macro_rules! get_os_release {
    () => {
        match OS_RELEASE.as_ref() {
            Ok(release) => release,
            Err(why) => {
                error!("failed to get os release: {}", why);
                return ptr::null_mut();
            }
        }
    };
}

#[repr(C)]
pub struct DistinstOsRelease {
    bug_report_url:     *mut libc::c_char,
    home_url:           *mut libc::c_char,
    id_like:            *mut libc::c_char,
    id:                 *mut libc::c_char,
    name:               *mut libc::c_char,
    pretty_name:        *mut libc::c_char,
    privacy_policy_url: *mut libc::c_char,
    support_url:        *mut libc::c_char,
    version_codename:   *mut libc::c_char,
    version_id:         *mut libc::c_char,
}

impl DistinstOsRelease {
    pub unsafe fn from_os_release(release: &OsRelease) -> DistinstOsRelease {
        DistinstOsRelease {
            bug_report_url:     CString::new(release.bug_report_url.clone()).unwrap().into_raw(),
            home_url:           CString::new(release.home_url.clone()).unwrap().into_raw(),
            id_like:            CString::new(release.id_like.clone()).unwrap().into_raw(),
            id:                 CString::new(release.id.clone()).unwrap().into_raw(),
            name:               CString::new(release.name.clone()).unwrap().into_raw(),
            pretty_name:        CString::new(release.pretty_name.clone()).unwrap().into_raw(),
            privacy_policy_url: CString::new(release.privacy_policy_url.clone())
                .unwrap()
                .into_raw(),
            support_url:        CString::new(release.support_url.clone()).unwrap().into_raw(),
            version_codename:   CString::new(release.version_codename.clone()).unwrap().into_raw(),
            version_id:         CString::new(release.version_id.clone()).unwrap().into_raw(),
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_os_release_destroy(release: *mut DistinstOsRelease) {
    unsafe fn free_field(field: *mut libc::c_char) {
        if field.is_null() {
            error!("DistinstOsRelease field was to be destroyed even though it is null");
        } else {
            CString::from_raw(field);
        }
    }

    if release.is_null() {
        error!("DistinstOsRelease was to be destroyed even though it is null");
    } else {
        free_field((*release).bug_report_url);
        free_field((*release).home_url);
        free_field((*release).id_like);
        free_field((*release).id);
        free_field((*release).name);
        free_field((*release).pretty_name);
        free_field((*release).privacy_policy_url);
        free_field((*release).support_url);
        free_field((*release).version_codename);
        free_field((*release).version_id);
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_bug_report_url(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().bug_report_url.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_home_url(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().home_url.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_id_like(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().id_like.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_id(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().id.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_name(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().name.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_pretty_name(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().pretty_name.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_privacy_policy_url(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().privacy_policy_url.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_support_url(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().support_url.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_version_codename(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().version_codename.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_version_id(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().version_id.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_version(len: *mut libc::c_int) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = get_os_release!().version.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}
