use std::{
    fs, io,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default)]
pub struct Timezones {
    zones: Vec<Zone>,
}

impl Timezones {
    pub fn new() -> io::Result<Self> {
        let mut output = Timezones::default();

        for zone in fs::read_dir("/usr/share/zoneinfo/")? {
            let zone = zone?;
            let zone_path = zone.path();
            if zone_path.is_dir() {
                let zone_name = zone.file_name().into_string().unwrap();
                let mut regions = Vec::new();
                for region in zone_path.read_dir()? {
                    let region = region?;
                    let region_path = region.path();
                    let region_name = region.file_name().into_string().unwrap();
                    regions.push(Region { name: region_name, path: region_path });
                }

                regions.sort_unstable();
                output.zones.push(Zone { name: zone_name, regions })
            }
        }

        output.zones.sort_unstable();
        Ok(output)
    }

    pub fn zones(&self) -> &[Zone] { &self.zones }
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct Zone {
    name:    String,
    regions: Vec<Region>,
}

impl Zone {
    pub fn name(&self) -> &str { &self.name }

    pub fn regions(&self) -> &[Region] { &self.regions }
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct Region {
    name: String,
    path: PathBuf,
}

impl Region {
    pub fn name(&self) -> &str { &self.name }

    pub fn path(&self) -> &Path { &self.path }

    pub fn install(&self, dest: &Path) -> io::Result<()> {
        let timezone = dest.join("etc/timezone");
        fs::remove_file(&timezone)?;
        symlink(&self.path, &timezone)
    }
}
