use std::path::{Path, PathBuf};
use sysfs_class::{Block, SysClass};

/// Methods that all block devices share, whether they are partitions or disks.
///
/// This trait is required to implement other disk traits.
pub trait BlockDeviceExt {
    /// The sys path of the block device.
    fn sys_block_path(&self) -> PathBuf {
        let path = ["/sys/class/block/", &*self.get_device_name()].concat();
        PathBuf::from(path)
    }

    fn is_partition(&self) -> bool {
        self.sys_block_path().join("partition").exists()
    }

    /// Checks if the device is a read-only device.
    fn is_read_only(&self) -> bool {
        Block::from_path(&self.sys_block_path())
            .ok()
            .map_or(false, |block| block.ro().ok() == Some(1))
    }

    /// Checks if the device is a removable device.
    ///
    /// # Notes
    /// This is only applicable for disk devices.
    fn is_removable(&self) -> bool {
        Block::from_path(&self.sys_block_path())
            .ok()
            .map_or(false, |block| block.removable().ok() == Some(1))
    }

    /// Checks if the device is a rotational device.
    ///
    /// # Notes
    /// This is only applicable for disk devices.
    fn is_rotational(&self) -> bool {
        Block::from_path(&self.sys_block_path())
            .ok()
            .map_or(false, |block| block.queue_rotational().ok() == Some(1))
    }

    /// The path to the block device, such as `/dev/sda1`, or `/dev/data/root`.
    fn get_device_path(&self) -> &Path;

    /// The mount point of this block device, if it is mounted.
    fn get_mount_point(&self) -> &[PathBuf] { &[] }

    /// The name of the device, such as `sda1`.
    fn get_device_name(&self) -> String {
        let device_path = self.get_device_path();
        let resolved = device_path.read_link();

        let name = match resolved.as_ref() {
            Ok(resolved) => resolved.file_name(),
            _ => device_path.file_name(),
        };

        name.expect("BlockDeviceExt::get_device_path missing file_name")
            .to_str()
            .expect("BlockDeviceExt::get_device_path invalid file_name")
            .to_owned()
    }

    fn get_parent_device(&self) -> Option<Block> {
        self.sys_block_path()
            .canonicalize()
            .ok()
            .and_then(|canon| {
                canon.parent()
                    .and_then(|parent| parent.file_name())
                    .and_then(|name| name.to_str())
                    .map(|parent| Path::new("/sys/class/block").join(parent))
            })
            .and_then(|parent| Block::from_path(&parent).ok())
    }
}
