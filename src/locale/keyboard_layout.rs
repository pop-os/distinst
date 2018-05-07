use serde_xml_rs::deserialize;
use std::io::{self, BufReader};
use std::fs::File;

#[derive(Debug, Deserialize)]
pub struct KeyboardLayouts {
    #[serde(rename = "layoutList")]
    layout_list: LayoutList,
}

impl KeyboardLayouts {
    pub fn get_layouts(&self) -> &[KeyboardLayout] {
        &self.layout_list.layout
    }

    pub fn get_layouts_mut(&mut self) -> &mut [KeyboardLayout] {
        &mut self.layout_list.layout
    }
}

#[derive(Debug, Deserialize)]
pub struct LayoutList {
    layout: Vec<KeyboardLayout>
}

#[derive(Debug, Deserialize)]
pub struct KeyboardLayout {
    #[serde(rename = "configItem")]
    config_item: ConfigItem,
    #[serde(rename = "variantList")]
    variant_list: Option<VariantList>,
}

impl KeyboardLayout {
    pub fn get_name(&self) -> &str {
        &self.config_item.name
    }

    pub fn get_description(&self) -> &str {
        &self.config_item.description
    }

    pub fn get_variants(&self) -> Option<&Vec<KeyboardVariant>> {
        self.variant_list.as_ref().and_then(|x| x.variant.as_ref())
    }
}


#[derive(Debug, Deserialize)]
pub struct ConfigItem {
    name: String,
    #[serde(rename = "shortDescription")]
    short_description: Option<String>,
    description: String,
}


#[derive(Debug, Deserialize)]
pub struct VariantList {
    variant: Option<Vec<KeyboardVariant>>
}


#[derive(Debug, Deserialize)]
pub struct KeyboardVariant {
    #[serde(rename = "configItem")]
    config_item: ConfigItem,
}

impl KeyboardVariant {
    pub fn get_name(&self) -> &str {
        &self.config_item.name
    }

    pub fn get_description(&self) -> &str {
        &self.config_item.description
    }
}

const X11_BASE_RULES: &'static str = "/usr/share/X11/xkb/rules/base.xml";

pub fn get_keyboard_layouts() -> io::Result<KeyboardLayouts> {
    deserialize(BufReader::new(File::open(X11_BASE_RULES)?))
        .map_err(|why| io::Error::new(io::ErrorKind::InvalidData, format!("{}", why)))
}
