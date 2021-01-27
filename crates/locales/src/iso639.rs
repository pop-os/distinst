use once_cell::sync::Lazy;
use serde_json::from_reader;
use std::collections::HashMap;
use std::fs::File;

const JSON_PATH_3: &str = "/usr/share/iso-codes/json/iso_639-3.json";
const JSON_PATH_5: &str = "/usr/share/iso-codes/json/iso_639-5.json";

#[derive(Debug, Deserialize)]
pub struct Language {
    alpha_2: Option<String>,
    alpha_3: String,
    pub name: String,
}

impl Language {
    pub fn all() -> &'static [Self] {
        static LANGUAGES: Lazy<Vec<Language>> = Lazy::new(|| {
            let f = File::open(JSON_PATH_3).unwrap();
            let mut m: HashMap<String, Vec<Language>> = from_reader(f).unwrap();
            let mut languages = m.remove("639-3").unwrap();

            // Language families, like Berber, which is needed
            let f = File::open(JSON_PATH_5).unwrap();
            let mut m: HashMap<String, Vec<Language>> = from_reader(f).unwrap();
            languages.extend(m.remove("639-5").unwrap());

            languages
        });
        Lazy::force(&LANGUAGES).as_slice()
    }

    pub fn from_alpha_2(alpha_2: &str) -> Option<&'static Self> {
        Self::all().iter().find(|i| i.alpha_2.as_ref().map(String::as_str) == Some(alpha_2))
    }

    pub fn from_alpha_3(alpha_3: &str) -> Option<&'static Self> {
        Self::all().iter().find(|i| i.alpha_3 == alpha_3)
    }
}
