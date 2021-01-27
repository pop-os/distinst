use once_cell::sync::Lazy;
use serde_json::from_reader;
use std::collections::HashMap;
use std::fs::File;

const JSON_PATH: &str = "/usr/share/iso-codes/json/iso_3166-1.json";

#[derive(Debug, Deserialize)]
pub struct Country {
    alpha_2: String,
    alpha_3: String,
    name: String,
    numeric: String,
    official_name: Option<String>,
    common_name: Option<String>,
}

impl Country {
    pub fn all() -> &'static [Self] {
        static COUNTRIES: Lazy<Vec<Country>> = Lazy::new(|| {
            let f = File::open(JSON_PATH).unwrap();
            let mut m: HashMap<String, Vec<Country>> = from_reader(f).unwrap();
            m.remove("3166-1").unwrap()
        });
        Lazy::force(&COUNTRIES).as_slice()
    }

    pub fn from_alpha_2(alpha_2: &str) -> Option<&'static Self> {
        Self::all().iter().find(|i| i.alpha_2 == alpha_2)
    }

    pub fn common_name(&self) -> &str {
        self.common_name.as_ref().unwrap_or(&self.name)
    }
}
