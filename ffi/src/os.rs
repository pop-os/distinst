use distinst::os_release::OS_RELEASE;
use libc;
use std::mem::forget;
use std::ptr;
use super::null_check;

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_bug_report_url(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.bug_report_url.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_home_url(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.home_url.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_id_like(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.id_like.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_id(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.id.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_name(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.name.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_pretty_name(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.pretty_name.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_privacy_policy_url(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.privacy_policy_url.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_support_url(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.support_url.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_version_codename(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.version_codename.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_version_id(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.version_id.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_version(
    len: *mut libc::c_int,
) -> *mut u8 {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let output = OS_RELEASE.version.clone();
    let output = output.into_bytes().into_boxed_slice();
    *len = output.len() as i32;
    let ptr = output.as_ptr() as *mut u8;
    forget(output);
    ptr
}
