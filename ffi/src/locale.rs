use super::{get_str, null_check, to_cstr};
use distinst::locale;
use libc;
use std::ptr;

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_default(
    lang: *const libc::c_char,
) -> *mut libc::c_char {
    get_str(lang)
        .ok()
        .and_then(|lang| locale::get_default(lang).map(to_cstr))
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_country_codes(
    lang: *const libc::c_char,
    len: *mut libc::c_int,
) -> *mut *mut libc::c_char {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    match get_str(lang).ok() {
        Some(lang) => {
            let mut output: Vec<*mut libc::c_char> = Vec::new();
            for country in locale::get_countries(lang) {
                output.push(to_cstr(country.into()));
            }

            *len = output.len() as libc::c_int;
            Box::into_raw(output.into_boxed_slice()) as *mut *mut libc::c_char
        }
        None => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_language_codes(
    len: *mut libc::c_int,
) -> *mut *mut libc::c_char {
    if null_check(len).is_err() {
        return ptr::null_mut();
    }

    let codes = locale::LOCALES.keys().cloned().map(to_cstr).collect::<Vec<*mut libc::c_char>>();

    *len = codes.len() as libc::c_int;
    Box::into_raw(codes.into_boxed_slice()) as *mut *mut libc::c_char
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_language_name(
    code: *const libc::c_char,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(len).is_err() {
        return ptr::null();
    }

    match get_str(code).ok().and_then(locale::get_language_name) {
        Some(code) => {
            *len = code.len() as libc::c_int;
            code.as_bytes().as_ptr()
        }
        None => ptr::null(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_language_name_translated(
    code: *const libc::c_char,
) -> *mut libc::c_char {
    get_str(code)
        .ok()
        .and_then(locale::get_language_name_translated)
        .map(to_cstr)
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_country_name(
    code: *const libc::c_char,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(len).is_err() {
        return ptr::null();
    }

    match get_str(code).ok().and_then(locale::get_country_name) {
        Some(code) => {
            *len = code.len() as libc::c_int;
            code.as_bytes().as_ptr()
        }
        None => ptr::null(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_country_name_translated(
    country_code: *const libc::c_char,
    lang_code: *const libc::c_char,
) -> *mut libc::c_char {
    get_str(country_code)
        .and_then(|x| get_str(lang_code).map(|y| (x, y)))
        .ok()
        .and_then(|(country, lang)| locale::get_country_name_translated(country, lang))
        .map(to_cstr)
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_main_country(
    code: *const libc::c_char,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(len).is_err() {
        return ptr::null();
    }

    match get_str(code).ok().and_then(locale::get_main_country) {
        Some(code) => {
            *len = code.len() as libc::c_int;
            code.as_bytes().as_ptr()
        }
        None => ptr::null(),
    }
}
