extern crate distinst;

use distinst::locale::*;

fn main() {
    for lang_code in get_language_codes() {
        println!(
            "{}: {:?} => {:?}: (default: {:?})",
            lang_code,
            get_language_name(lang_code),
            get_language_name_translated(lang_code),
            get_default(lang_code)
        );

        for country_code in get_countries(lang_code) {
            println!(
                "    {}: {:?} => {:?}",
                country_code,
                get_country_name(country_code),
                get_country_name_translated(country_code, lang_code)
            );
        }
    }
}
