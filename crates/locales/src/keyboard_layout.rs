use misc;
use serde_xml_rs as xml;
use std::io::{self, BufReader};

/// A list of keyboard layouts parsed from `/usr/share/X11/xkb/rules/base.xml`.
#[derive(Debug, Deserialize)]
pub struct KeyboardLayouts {
    #[serde(rename = "layoutList")]
    pub layout_list: LayoutList,
}

impl KeyboardLayouts {
    /// Fetch the layouts from the layout list.
    pub fn get_layouts(&self) -> &[KeyboardLayout] { &self.layout_list.layout }

    /// Fetch the layouts from the layout list.
    pub fn get_layouts_mut(&mut self) -> &mut [KeyboardLayout] { &mut self.layout_list.layout }
}

/// A list of keyboard layouts.
#[derive(Debug, Deserialize)]
pub struct LayoutList {
    pub layout: Vec<KeyboardLayout>,
}

/// A keyboard layout, which contains an optional list of variants, a name, and a description.
#[derive(Debug, Deserialize)]
pub struct KeyboardLayout {
    #[serde(rename = "configItem")]
    pub config_item:  ConfigItem,
    #[serde(rename = "variantList")]
    pub variant_list: Option<VariantList>,
}

impl KeyboardLayout {
    /// Fetches the name of the keyboard layout.
    pub fn get_name(&self) -> &str { &self.config_item.name }

    /// Fetches a description of the layout.
    pub fn get_description(&self) -> &str { &self.config_item.description }

    /// Fetches a list of possible layout variants.
    pub fn get_variants(&self) -> Option<&Vec<KeyboardVariant>> {
        self.variant_list.as_ref().and_then(|x| x.variant.as_ref())
    }
}

/// Contains the name and description of a keyboard layout.
#[derive(Debug, Deserialize)]
pub struct ConfigItem {
    pub name:              String,
    #[serde(rename = "shortDescription")]
    pub short_description: Option<String>,
    pub description:       String,
}

/// A list of possible variants of a keyboard layout.
#[derive(Debug, Deserialize)]
pub struct VariantList {
    pub variant: Option<Vec<KeyboardVariant>>,
}

/// A variant of a keyboard layout.
#[derive(Debug, Deserialize)]
pub struct KeyboardVariant {
    #[serde(rename = "configItem")]
    pub config_item: ConfigItem,
}

impl KeyboardVariant {
    /// The name of this variant of a keybaord layout.
    pub fn get_name(&self) -> &str { &self.config_item.name }

    /// A description of this variant of a keyboard layout.
    pub fn get_description(&self) -> &str { &self.config_item.description }
}

const X11_BASE_RULES: &str = "/usr/share/X11/xkb/rules/base.xml";

/// Fetches a list of keyboard layouts from `/usr/share/X11/xkb/rules/base.xml`.
pub fn get_keyboard_layouts() -> io::Result<KeyboardLayouts> {
    xml::from_reader(BufReader::new(misc::open(X11_BASE_RULES)?))
        .map_err(|why| io::Error::new(io::ErrorKind::InvalidData, format!("{}", why)))
}
