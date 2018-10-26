use std::path::{Path, PathBuf};
use sysfs_class::{Block, SysClass};

pub trait BlockDeviceExt {
    /// Checks if the drive is a removable drive.
    fn is_removable(&self) -> bool {
        let path = {
            let path = self.get_device_path();
            PathBuf::from(match path.read_link() {
                Ok(resolved) => [
                    "/sys/class/block/",
                    resolved.file_name().expect("drive does not have a file name").to_str().unwrap(),
                ].concat(),
                _ => [
                    "/sys/class/block/",
                    path.file_name().expect("drive does not have a file name").to_str().unwrap(),
                ].concat(),
            })
        };

        Block::from_path(&Path::new(&path))
            .ok()
            .map_or(false, |block| block.removable().ok() == Some(1))
    }

    /// Where this block device originates.
    fn get_device_path(&self) -> &Path;

    /// The mount point of this block device, if it is mounted.
    fn get_mount_point(&self) -> Option<&Path>;
}
