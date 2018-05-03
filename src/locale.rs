//! Reads a list of supported locales from /usr/share/i18n/SUPPORTED into a map. Using that map
//! this provides convenience methods for getting default locales and a list of countries
//! associated with a language (if any exist at all).

use std::io::{self, BufRead, BufReader};
use std::fs::File;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;

lazy_static! {
    pub static ref LOCALES: Locales = parse_locales().unwrap();
}

pub type Locales = HashMap<String, (Vec<Option<String>>, Vec<Option<(String, bool)>>)>;

pub fn get_default(lang: &String) -> Option<String> {
    LOCALES.get(lang)
        .map(|value| {
            let country_value = match value.0.iter().next() {
                Some(country) => country,
                None => {
                    return lang.clone();
                }
            };

            match (country_value, value.1.iter().next()) {
                (&Some(ref country), Some(&Some((ref unicode, dot)))) if dot => {
                    format!("{}_{}.{}", lang, country, unicode)
                }
                (&Some(ref country), Some(&Some((ref unicode, _)))) => {
                    format!("{}_{} {}", lang, country, unicode)
                }
                (&Some(ref country), Some(&None)) => {
                    format!("{}_{}", lang, country)
                }
                (&Some(ref country), None) => {
                    format!("{}_{}", lang, country)
                }
                (&None, Some(&Some((ref unicode, dot)))) if dot => {
                    format!("{}.{}", lang, unicode)
                }
                (&None, Some(&Some((ref unicode, _)))) => {
                    format!("{} {}", lang, unicode)
                }
                _ => {
                    lang.clone()
                }
            }
        })
}

pub fn get_countries(lang: &String) -> Vec<String> {
    match LOCALES.get(lang) {
        Some(value) => {
            value.1.iter()
                .filter_map(|country| country.clone().map(|x| x.0))
                .collect()
        }
        None => Vec::new()
    }
}

pub fn parse_locales() -> io::Result<Locales> {
    let mut locales = HashMap::new();
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
                            let value: &mut (Vec<Option<String>>, Vec<Option<(String, bool)>>) = entry.get_mut();
                            value.0.push(country);
                            value.1.push(unicode);
                        }
                        Entry::Vacant(entry) => {
                            entry.insert((vec![country], vec![unicode]));
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
                            Some(code) => (lang.into(), Some(country.into()), Some((code.into(), false))),
                            None => (lang.into(), Some(country.into()), None),
                        },
                        (Some(country), Some(code)) => (lang.into(), Some(country.into()), Some((code.into(), true)))
                    }
                }
            })
        }
        None => None
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
