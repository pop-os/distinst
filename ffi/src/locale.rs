use libc;
use distinst::locale;
use super::{get_str, to_cstr};
use std::ptr;

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_default(lang: *const libc::c_char) -> *mut libc::c_char {
    get_str(lang, "")
        .ok()
        .and_then(|lang| locale::get_default(&lang.into()).map(to_cstr))
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_countries(
    lang: *const libc::c_char,
    len: *mut libc::c_int,
) -> *mut *mut libc::c_char {
    match get_str(lang, "").ok() {
        Some(lang) => {
            let mut output: Vec<*mut libc::c_char> = Vec::new();
            for country in locale::get_countries(&lang.into()) {
                output.push(to_cstr(country));
            }

            *len = output.len() as libc::c_int;
            Box::into_raw(output.into_boxed_slice()) as *mut *mut libc::c_char
        }
        None => ptr::null_mut()
    }
}
