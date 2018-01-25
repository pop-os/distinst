use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub struct Mounts(Vec<MountInfo>);

impl Mounts {
    pub fn new() -> io::Result<Mounts> {
        let mut mounts = File::open("/proc/mounts")?;
        let mut buffer =
            String::with_capacity(mounts.metadata().ok().map_or(0, |m| m.len() as usize));
        let _ = mounts.read_to_string(&mut buffer)?;

        let mut mounts = Vec::new();
        for mount in buffer.lines() {
            let mut fields = mount.split_whitespace();
            let device = fields.next().unwrap();

            // Skip devices which aren't tied to actual hardware.
            if !device.contains('/') {
                continue;
            }

            mounts.push(MountInfo {
                device:      Path::new(&device).to_path_buf(),
                mount_point: Path::new(&fields.next().unwrap()).to_path_buf(),
            })
        }
        Ok(Mounts(mounts))
    }

    pub fn get_mount_point(&self, path: &Path) -> Option<PathBuf> {
        self.0
            .iter()
            .find(|mount| mount.device == path)
            .map(|mount| mount.mount_point.clone())
    }
}

pub struct MountInfo {
    device:      PathBuf,
    mount_point: PathBuf,
}
