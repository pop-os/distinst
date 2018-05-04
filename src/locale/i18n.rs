//! Reads a list of supported locales from /usr/share/i18n/SUPPORTED into a map. Using that map
//! this provides convenience methods for getting default locales and a list of countries
//! associated with a language (if any exist at all).

use std::io::{self, BufRead, BufReader};
use std::fs::File;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::Path;
use super::get_main_country;

lazy_static! {
    pub static ref LOCALES: Locales = parse_locales().unwrap();
}

pub type Language = String;
pub type Locales = BTreeMap<Language, Locale>;
pub type Locale = BTreeMap<Country, Codesets>;
pub type Country = Option<String>;
pub type Codesets = Vec<Option<(String, bool)>>;

pub fn get_default(lang: &str) -> Option<String> {
    LOCALES.get(lang)
        .map(|value| {
            if let Some(country) = get_main_country(lang) {
                return match value.get(&Some(country.into())) {
                    Some(codeset) => {
                        if codeset.contains(&Some(("UTF-8".into(), true))) {
                            format!("{}_{}.UTF-8", lang, country)
                        } else {
                            match codeset.first() {
                                Some(&Some((ref codeset, dot))) if dot => {
                                    format!("{}_{}.{}", lang, country, codeset)
                                }
                                _ => format!("{}_{}", lang, country)
                            }
                        }
                    }
                    None => format!("{}_{}", lang, country)
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
                None => lang.into()
            };

            if codeset.contains(&Some(("UTF-8".into(), true))) {
                format!("{}.UTF-8", prefix)
            } else {
                match codeset.first() {
                    Some(&Some((ref codeset, dot))) if dot => {
                        format!("{}.{}", prefix, codeset)
                    }
                    _ => prefix
                }
            }
        })
}

pub fn get_language_codes() -> Vec<&'static str> {
    LOCALES.keys().map(|x| x.as_str()).collect()
}

pub fn get_countries(lang: &str) -> Vec<&'static str> {
    match LOCALES.get(lang) {
        Some(value) => {
            value.keys()
                .flat_map(|c| c.as_ref().map(|x| x.as_str()))
                .collect()
        }
        None => Vec::new()
    }
}

pub fn parse_locales() -> io::Result<Locales> {
    let mut locales = BTreeMap::new();
    for file in &[Path::new("/usr/share/i18n/SUPPORTED"), Path::new("/usr/local/share/i18n/SUPPORTED")] {
        if !file.exists() {
            continue
        }
        for line in BufReader::new(File::open(file)?).lines() {
            let line = line?;
            match parse_entry(&line) {
                Some((lang, country, unicode)) => {
                    match locales.entry(lang) {
                        Entry::Occupied(mut entry) => {
                            let value: &mut Locale = entry.get_mut();
                            match value.entry(country) {
                                Entry::Occupied(mut entry) => {
                                    let entry = entry.get_mut();
                                    if !entry.contains(&unicode) {
                                        entry.push(unicode);
                                    }
                                }
                                Entry::Vacant(entry) => {
                                    entry.insert(vec![unicode]);
                                }
                            }
                        }
                        Entry::Vacant(entry) => {
                            let mut map = BTreeMap::new();
                            map.insert(country, vec![unicode]);
                            entry.insert(map);
                        }
                    }
                }
                None => ()
            }
        }
    }

    Ok(locales)
}

fn parse_entry(line: &str) -> Option<(String, Option<String>, Option<(String, bool)>)> {
    let mut words = line.split_whitespace();

    match words.next() {
        Some(word) => {
            let mut codes = word.split('_');
            Some(match (codes.next(), codes.next()) {
                (None, _) => {
                    return None;
                },
                (Some(lang), None) => (lang.into(), None, None),
                (Some(lang), Some(country)) => {
                    let mut codes = country.split('.');
                    match (codes.next(), codes.next()) {
                        (None, _) => (lang.into(), None, None),
                        (Some(country), None) => match words.next() {
                            Some(code) => (lang.into(), trim_into(country), Some((code.into(), false))),
                            None => (lang.into(), trim_into(country), None),
                        },
                        (Some(country), Some(code)) => (lang.into(), trim_into(country), Some((code.into(), true)))
                    }
                }
            })
        }
        None => None
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
            Some(("gu".into(), Some("IN".into()), Some(("UTF-8".into(), false))))
        );

        assert_eq!(
            parse_entry(&lines.next().unwrap()),
            Some(("gv".into(), Some("GB".into()), Some(("UTF-8".into(), true))))
        );

        assert_eq!(
            parse_entry(&lines.next().unwrap()),
            Some(("gv".into(), Some("GB".into()), Some(("ISO-8859-1".into(), false))))
        );

        assert_eq!(
            parse_entry(&lines.next().unwrap()),
            Some(("hak".into(), Some("TW".into()), Some(("UTF-8".into(), false))))
        );
    }
}
