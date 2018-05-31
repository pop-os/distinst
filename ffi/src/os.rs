use distinst::os_release::OS_RELEASE;
use libc;

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_bug_report_url(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.bug_report_url.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_home_url(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.home_url.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_id_like(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.id_like.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_id(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.id.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_name(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.name.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_pretty_name(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.pretty_name.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_privacy_policy_url(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.privacy_policy_url.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_support_url(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.support_url.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_version_codename(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.version_codename.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_version_id(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.version_id.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_get_os_version(
    len: *mut libc::c_int,
) -> *const u8 {
    let output = OS_RELEASE.version.as_bytes();
    *len = output.len() as i32;
    output.as_ptr()
}
