use misc;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
};

lazy_static! {
    /// A list of countries considered as "main countries" for a given language code.
    pub static ref MAIN_COUNTRIES: BTreeMap<String, String> = get_main_countries();
}

const MAIN_COUNTRIES_PATH: &str = "/usr/share/language-tools/main-countries";

/// Gets the main country for the given language code.
///
/// # Example
/// ```rust,no_run
/// use distinst_locale_support::get_main_country;
/// assert_eq!(get_main_country("en"), Some("US"));
/// ```
pub fn get_main_country(code: &str) -> Option<&'static str> {
    MAIN_COUNTRIES.get(code).map(|x| x.as_str())
}

/// Fetch a list of main countries, according to `/usr/share/language-tools/main-countries`.
pub fn get_main_countries() -> BTreeMap<String, String> {
    let file = match misc::open(MAIN_COUNTRIES_PATH) {
        Ok(file) => file,
        Err(why) => {
            eprintln!(
                "{:?} could not be opened: {}. returning empty collection.",
                MAIN_COUNTRIES_PATH, why
            );
            return BTreeMap::new();
        }
    };

    get_main_countries_iter(BufReader::new(file).lines().flatten())
}

fn get_main_countries_iter<I: Iterator<Item = String>>(iter: I) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();

    for line in iter.filter(|x| !x.starts_with('#')) {
        let mut fields = line.split_whitespace();
        if let (Some(code), Some(country)) = (fields.next(), fields.next()) {
            if let Some(country) = country.split('_').nth(1) {
                map.insert(code.into(), country.into());
            }
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"#
aa	aa_ET
ar	ar_EG
bn	bn_BD
"#;

    #[test]
    fn main_countries() {
        assert_eq!(get_main_countries_iter(EXAMPLE.lines().map(|x| x.into())), {
            let mut map = BTreeMap::new();
            map.insert("aa".into(), "ET".into());
            map.insert("ar".into(), "EG".into());
            map.insert("bn".into(), "BD".into());
            map
        });
    }
}
