use std::io::{self, Read};
use std::fs::File;

#[derive(Debug, PartialEq)]
pub struct Module {
    pub name: String,
}

impl Module {
    fn parse(line: &str) -> io::Result<Module> {
        let mut parts = line.split(" ");

        let name = parts.next().ok_or(io::Error::new(
            io::ErrorKind::InvalidData,
            "module name not found"
        ))?;

        Ok(Module {
            name: name.to_string(),
        })
    }

    pub fn parse_from<'a, I: Iterator<Item = &'a str>>(lines: I) -> io::Result<Vec<Module>> {
        lines.map(Self::parse).collect()
    }

    pub fn all() -> io::Result<Vec<Module>> {
        let file = File::open("/proc/modules")
            .and_then(|mut file| {
                let length = file.metadata().ok().map_or(0, |x| x.len() as usize);
                let mut string = String::with_capacity(length);
                file.read_to_string(&mut string).map(|_| string)
            })?;

        Self::parse_from(file.lines())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &'static str = r#"snd_hda_intel 40960 9 - Live 0x0000000000000000
snd_hda_codec 126976 4 snd_hda_codec_hdmi,snd_hda_codec_realtek,snd_hda_codec_generic,snd_hda_intel, Live 0x0000000000000000
snd_hda_core 81920 5 snd_hda_codec_hdmi,snd_hda_codec_realtek,snd_hda_codec_generic,snd_hda_intel,snd_hda_codec, Live 0x0000000000000000
nvidia_drm 40960 11 - Live 0x0000000000000000 (POE)
nvidia_modeset 1085440 20 nvidia_drm, Live 0x0000000000000000 (POE)
nvidia 14012416 938 nvidia_uvm,nvidia_modeset, Live 0x0000000000000000 (POE)
video 40960 0 - Live 0x0000000000000000 (E)"#;

    #[test]
    fn modules() {
        assert_eq!(
            Module::parse_from(SAMPLE.lines()).unwrap(),
            vec![
                Module { name: "snd_hda_intel".into() },
                Module { name: "snd_hda_codec".into() },
                Module { name: "snd_hda_core".into() },
                Module { name: "nvidia_drm".into() },
                Module { name: "nvidia_modeset".into() },
                Module { name: "nvidia".into() },
                Module { name: "video".into() },
            ]
        )
    }
}
