use libc;
use distinst::locale;
use super::{get_str, to_cstr};
use std::ptr;

cstr_methods!(
    fn distinst_locale_get_default(code) {
        locale::get_default(code)
    }

    fn distinst_locale_get_main_country(code) {
        locale::get_main_country(code)
    }
    fn distinst_locale_get_language_name(code) {
        locale::get_language_name(code)
    }

    fn distinst_locale_get_language_name_translated(code) {
        locale::get_language_name_translated(code)
    }

    fn distinst_locale_get_country_name(code) {
        locale::get_country_name(code)
    }
);

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_country_codes(
    lang: *const libc::c_char,
    len: *mut libc::c_int,
) -> *mut *mut libc::c_char {
    match get_str(lang, "").ok() {
        Some(lang) => {
            cvec_from!{
                for country in locale::get_countries(lang),
                    push to_cstr(country.into()),
                    record len
            }
        }
        None => ptr::null_mut()
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_language_codes(
    len: *mut libc::c_int
) -> *mut *mut libc::c_char {
    cvec_from!{
        for code in locale::LOCALES.keys().cloned(),
            push to_cstr(code),
            record len
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_locale_get_country_name_translated(
    country_code: *const libc::c_char,
    lang_code: *const libc::c_char,
) -> *mut libc::c_char {
    get_str(country_code, "")
        .and_then(|x| get_str(lang_code, "").map(|y| (x, y)))
        .ok()
        .and_then(|(country, lang)| locale::get_country_name_translated(country, lang))
        .map(to_cstr)
        .unwrap_or(ptr::null_mut())
}
