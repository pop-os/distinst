use std::path::{Path, PathBuf};
use sysfs_class::{Block, SysClass};

pub trait BlockDeviceExt {
    fn sys_block_path(&self) -> PathBuf {
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
    }

    /// Checks if the device is a removable device.
    fn is_removable(&self) -> bool {
        Block::from_path(&self.sys_block_path())
            .ok()
            .map_or(false, |block| block.removable().ok() == Some(1))
    }

    /// Checks if the device is a rotational device.
    fn is_rotational(&self) -> bool {
        Block::from_path(&self.sys_block_path())
            .ok()
            .map_or(false, |block| block.queue_rotational().ok() == Some(1))
    }

    /// Where this block device originates.
    fn get_device_path(&self) -> &Path;

    /// The mount point of this block device, if it is mounted.
    fn get_mount_point(&self) -> Option<&Path>;
}
