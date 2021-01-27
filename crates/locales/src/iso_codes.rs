use super::get_default;
use gettextrs::*;
use iso3166_1::{alpha2 as iso_3166_1, CountryCode};
use isolang::Language;
use std::env;

/// Fetch the ISO 639 name of a language code.
pub fn get_language_name(code: &str) -> Option<&'static str> {
    match code.len() {
        2 => Language::from_639_1(code).map(|x| x.to_name()),
        3 => Language::from_639_3(code).map(|x| x.to_name()),
        _ => None,
    }
}

/// Fetch a translated version of that name, based on the given language code.
pub fn get_language_name_translated(code: &str) -> Option<String> {
    let current_lang = env::var("LANGUAGE");
    if let Some(locale) = get_default(code) {
        env::set_var("LANGUAGE", locale);
    }
    setlocale(LocaleCategory::LcAll, "");

    let result = get_language_name(code).map(|language_name| dgettext("iso_639_3", language_name));

    match current_lang {
        Ok(lang) => env::set_var("LANGUAGE", lang),
        _ => env::remove_var("LANGUAGE"),
    }

    result
}

/// Get the country name of an ISO 3166 country code.
pub fn get_country_name(code: &str) -> Option<&'static str> {
    if code == "TW" || code == "TWN" {
        Some("Taiwan")
    } else {
        iso_3166_1(code).map(|x| x.name)
    }
}

/// Get a country code from an ISO 3166 country code.
pub fn get_country(code: &str) -> Option<CountryCode> { iso_3166_1(code) }

/// Get the country name translated into the given language code.
pub fn get_country_name_translated(country_code: &str, lang_code: &str) -> Option<String> {
    get_country_name(country_code).map(|country| {
        let current_lang = env::var("LANGUAGE");
        if let Some(locale) = get_default(lang_code) {
            env::set_var("LANGUAGE", locale);
        }

        setlocale(LocaleCategory::LcAll, "");
        let result = dgettext("iso_3166", country);

        match current_lang {
            Ok(lang) => env::set_var("LANGUAGE", lang),
            _ => env::remove_var("LANGUAGE"),
        }

        result
    })
}
