use distinst::os_release::OS_RELEASE;
use libc;
use std::mem::forget;
use std::ptr;
use super::null_check;

macro_rules! get_os_release {
    () => {
        match OS_RELEASE.as_ref() {
            Ok(release) => release,
            Err(why) => {
                error!("failed to get os release: {}", why);
                return ptr::null_mut();
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_bug_report_url(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_home_url(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_id_like(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_id(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_name(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_pretty_name(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_privacy_policy_url(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_support_url(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_version_codename(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_version_id(
    len: *mut libc::c_int,
) -> *mut u8 {
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
pub unsafe extern "C" fn distinst_get_os_version(
    len: *mut libc::c_int,
) -> *mut u8 {
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
