//! Reads a list of supported locales from /usr/share/i18n/SUPPORTED into a map. Using that map
//! this provides convenience methods for getting default locales and a list of countries
//! associated with a language (if any exist at all).

use super::get_main_country;
use misc;
use std::{
    collections::{btree_map::Entry, BTreeMap},
    io::{self, BufRead, BufReader},
    path::Path,
};

lazy_static! {
    /// Stores the parsed supported locales from `/usr/share/i18n/SUPPORTED`.
    pub static ref LOCALES: Locales = parse_locales().unwrap();
}

/// An ISO 631 language code
pub type Language = String;
/// A listing of supported locales.
pub type Locales = BTreeMap<Language, Locale>;
/// A language locale, which contains a list of possible countries and codesets.
pub type Locale = BTreeMap<Country, Codesets>;
/// A country code, which may or may not exist.
pub type Country = Option<String>;
/// Code sets for a locale, if they exist.
pub type Codesets = Vec<Option<Codeset>>;

/// Fetch the default country for a given language, if it exists.
pub fn get_default(lang: &str) -> Option<String> {
    LOCALES.get(lang).map(|value| {
        if let Some(country) = get_main_country(lang) {
            return match value.get(&Some(country.into())) {
                Some(codeset) => {
                    if codeset.contains(&Some(Codeset::new("UTF-8".into(), true))) {
                        format!("{}_{}.UTF-8", lang, country)
                    } else {
                        match codeset.first() {
                            Some(&Some(ref codeset)) if codeset.dot => {
                                format!("{}_{}.{}", lang, country, codeset.variant)
                            }
                            _ => format!("{}_{}", lang, country),
                        }
                    }
                }
                None => format!("{}_{}", lang, country),
            };
        }

        let (country, codeset) = match value.iter().next() {
            Some(value) => value,
            None => {
                return lang.into();
            }
        };

        let prefix = match *country {
            Some(ref country) => format!("{}_{}", lang, country),
            None => lang.into(),
        };

        if codeset.contains(&Some(Codeset::new("UTF-8".into(), true))) {
            format!("{}.UTF-8", prefix)
        } else {
            match codeset.first() {
                Some(&Some(ref codeset)) if codeset.dot => {
                    format!("{}.{}", prefix, codeset.variant)
                }
                _ => prefix,
            }
        }
    })
}

/// Fetch a list of language codes.
///
/// Equivalent to:
///
/// ```rust,no_exec,no_run
/// LOCALES.keys().map(|x| x.as_str()).collect()
/// ```
pub fn get_language_codes() -> Vec<&'static str> { LOCALES.keys().map(|x| x.as_str()).collect() }

/// Fetch a list of countries associated with a language code.
pub fn get_countries(lang: &str) -> Vec<&'static str> {
    match LOCALES.get(lang) {
        Some(value) => value.keys().map(|c| c.as_ref().map_or("None", |x| x.as_str())).collect(),
        None => Vec::new(),
    }
}

/// Reads and parses the contents of `/usr/share/i18n/SUPPORTED` for a list of supported
/// languages and their possible country codes.
pub fn parse_locales() -> io::Result<Locales> {
    let mut locales = BTreeMap::new();
    for file in
        &[Path::new("/usr/share/i18n/SUPPORTED"), Path::new("/usr/local/share/i18n/SUPPORTED")]
    {
        if !file.exists() {
            continue;
        }
        for line in BufReader::new(misc::open(file)?).lines() {
            let line = line?;
            if let Some(locale) = parse_entry(&line) {
                match locales.entry(locale.language) {
                    Entry::Occupied(mut entry) => {
                        let value: &mut Locale = entry.get_mut();
                        match value.entry(locale.country) {
                            Entry::Occupied(mut entry) => {
                                let entry = entry.get_mut();
                                if !entry.contains(&locale.codeset) {
                                    entry.push(locale.codeset);
                                }
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(vec![locale.codeset]);
                            }
                        }
                    }
                    Entry::Vacant(entry) => {
                        let mut map = BTreeMap::new();
                        map.insert(locale.country, vec![locale.codeset]);
                        entry.insert(map);
                    }
                }
            }
        }
    }

    Ok(locales)
}

/// A locale, which contains a language, an optional country, and an optional codeset.
#[derive(Debug, PartialEq)]
struct LocaleEntry {
    language: String,
    country:  Option<String>,
    codeset:  Option<Codeset>,
}

impl LocaleEntry {
    fn new(
        language: String,
        country: Option<String>,
        codeset: Option<(String, bool)>,
    ) -> LocaleEntry {
        LocaleEntry {
            language,
            country,
            codeset: codeset.map(|c| Codeset { variant: c.0, dot: c.1 }),
        }
    }
}

/// The codeset of a locale ie: `.UTF-8`
#[derive(Debug, PartialEq)]
pub struct Codeset {
    pub variant: String,
    pub dot:     bool,
}

impl Codeset {
    fn new(variant: String, dot: bool) -> Codeset { Codeset { variant, dot } }
}

fn parse_entry(line: &str) -> Option<LocaleEntry> {
    let mut words = line.split_whitespace();

    match words.next() {
        Some(word) => {
            let mut codes = word.split('_');
            Some(match (codes.next(), codes.next()) {
                (None, _) => {
                    return None;
                }
                (Some(lang), None) => LocaleEntry::new(lang.into(), None, None),
                (Some(lang), Some(country)) => {
                    let mut codes = country.split('.');
                    match (codes.next(), codes.next()) {
                        (None, _) => LocaleEntry::new(lang.into(), None, None),
                        (Some(country), None) => match words.next() {
                            Some(code) => LocaleEntry::new(
                                lang.into(),
                                trim_into(country),
                                Some((code.into(), false)),
                            ),
                            None => LocaleEntry::new(lang.into(), trim_into(country), None),
                        },
                        (Some(country), Some(code)) => LocaleEntry::new(
                            lang.into(),
                            trim_into(country),
                            Some((code.into(), true)),
                        ),
                    }
                }
            })
        }
        None => None,
    }
}

fn trim_into(input: &str) -> Option<String> {
    let input = input.trim();
    if input.is_empty() || input.contains('@') {
        None
    } else {
        Some(input.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUT: &str = r#"gu_IN UTF-8
gv_GB.UTF-8 UTF-8
gv_GB ISO-8859-1
hak_TW UTF-8
"#;
    #[test]
    fn locales() {
        let mut lines = INPUT.lines();

        assert_eq!(
            parse_entry(&lines.next().unwrap()),
            Some(LocaleEntry::new("gu".into(), Some("IN".into()), Some(("UTF-8".into(), false))))
        );

        assert_eq!(
            parse_entry(&lines.next().unwrap()),
            Some(LocaleEntry::new("gv".into(), Some("GB".into()), Some(("UTF-8".into(), true))))
        );

        assert_eq!(
            parse_entry(&lines.next().unwrap()),
            Some(LocaleEntry::new(
                "gv".into(),
                Some("GB".into()),
                Some(("ISO-8859-1".into(), false))
            ))
        );

        assert_eq!(
            parse_entry(&lines.next().unwrap()),
            Some(LocaleEntry::new("hak".into(), Some("TW".into()), Some(("UTF-8".into(), false))))
        );
    }
}
