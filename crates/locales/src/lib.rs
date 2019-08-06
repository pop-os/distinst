//! Provides the locale support required by distinst and distinst-based installers. Locales
//! include keyboard layouts, language and country codes.

extern crate distinst_utils as misc;
extern crate gettextrs;
extern crate iso3166_1;
extern crate isolang;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
extern crate serde_xml_rs;

mod i18n;
mod iso_codes;
mod keyboard_layout;
mod main_countries;

pub use self::{i18n::*, iso_codes::*, keyboard_layout::*, main_countries::*};
