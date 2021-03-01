use std::{
    cell::RefCell,
    fmt::Debug,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    str::FromStr,
};
use sysfs_class::{Block, SysClass};

/// Methods that all block devices share, whether they are partitions or disks.
///
/// This trait is required to implement other disk traits.
pub trait BlockDeviceExt {
    /// The sys path of the block device.
    fn sys_block_path(&self) -> PathBuf { sys_block_path(self.get_device_name(), "") }

    fn is_partition(&self) -> bool {
        sys_block_path(self.get_device_name(), "partition").exists()
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
    fn get_mount_point(&self) -> Option<&Path> { None }

    /// The name of the device, such as `sda1`.
    fn get_device_name(&self) -> &str {
        self.get_device_path()
            .file_name()
            .expect("BlockDeviceExt::get_device_path missing file_name")
            .to_str()
            .expect("BlockDeviceExt::get_device_path invalid file_name")
    }

    /// The combined total number of sectors on the disk.
    fn get_sectors(&self) -> u64 {
        let size_file = sys_block_path(self.get_device_name(), "/size");
        read_file::<u64>(&size_file).expect("no sector count found")
    }

    /// The size of each logical sector, in bytes.
    fn get_logical_block_size(&self) -> u64 {
        eprintln!("get block size for {:?}", self.sys_block_path());

        let block = Block::from_path(&self.sys_block_path())
            .expect("device lacks block");

        match block.queue_logical_block_size() {
            Ok(size) => return size,
            Err(_) => {
                return Block::from_path(&self.sys_block_path())
                    .expect("partition does not have a block device")
                    .parent_device()
                    .expect("partition lacks parent block device")
                    .queue_logical_block_size()
                    .expect("parent of partition lacks logical block size");
            }
        }
    }

    /// The size of each logical sector, in bytes.
    fn get_physical_block_size(&self) -> u64 {
        let path = sys_block_path(self.get_device_name(), "/queue/physical_block_size");
        read_file::<u64>(&path).expect("physical block size not found")
    }
}

fn sys_block_path(name: &str, ppath: &str) -> PathBuf {
    PathBuf::from(["/sys/class/block/", name, ppath].concat())
}

thread_local! {
    static BUFFER: RefCell<String> = String::with_capacity(256).into();
}

fn read_file<T: FromStr>(path: &Path) -> io::Result<T>
where
    <T as FromStr>::Err: Debug,
{
    BUFFER.with(|buffer| {
        let mut buffer = buffer.borrow_mut();
        File::open(path)?.read_to_string(&mut buffer)?;
        let value = buffer.trim().parse::<T>();
        buffer.clear();
        value.map_err(|why| io::Error::new(io::ErrorKind::Other, format!("{:?}", why)))
    })
}
