use device::BlockDeviceExt;
use fs::FileSystem;
use fs::FileSystem::*;
use libparted::PartitionFlag;
use usage::sectors_used;
use tempdir::TempDir;
use sys_mount::*;
use std::io;
use std::path::Path;
use os_detect::{detect_os, OS};

pub trait PartitionExt: BlockDeviceExt {
    /// True if the partition is a swap partition.
    fn is_swap(&self) -> bool {
        self.get_file_system()
            .map_or(false, |fs| fs == FileSystem::Swap)
    }

    /// True if the partition is compatible for Linux to be installed on it.
    fn is_linux_compatible(&self) -> bool {
        self.get_file_system()
            .map_or(false, |fs| match fs {
                Exfat | Ntfs | Fat16 | Fat32 | Lvm | Luks | Swap => false,
                Btrfs | Xfs | Ext2 | Ext3 | Ext4 | F2fs => true
            })
    }

    /// Defines the file system that this device was partitioned with.
    fn get_file_system(&self) -> Option<FileSystem>;

    /// Ped partition flags that this disk has been assigned.
    fn get_partition_flags(&self) -> &[PartitionFlag];

    /// The label of the partition, if it has one.
    fn get_partition_label(&self) -> Option<&str>;

    /// Whether this partition is primary, logical, or extended.
    fn get_partition_type(&self) -> PartitionType;

    /// The sector where this partition ends on the parent block device.
    fn get_sector_end(&self) -> u64;

    /// The sector where this partition begins on the parent block device..
    fn get_sector_start(&self) -> u64;

    /// Returns the length of the partition in sectors.
    fn get_sectors(&self) -> u64 { self.get_sector_end() - self.get_sector_start() }

    /// Mount the file system at a temporary directory, and allow the caller to scan it.
    fn probe<T, F>(&self, mut func: F) -> T
        where F: FnMut(Option<(&Path, UnmountDrop<Mount>)>) -> T
    {
        let mount = self.get_file_system()
            .and_then(|fs| TempDir::new("distinst").ok().map(|t| (fs, t)));

        if let Some((fs, tempdir)) = mount {
            let fs = match fs {
                FileSystem::Fat16 | FileSystem::Fat32 => "vfat",
                fs => fs.into(),
            };

            // Mount the FS to the temporary directory
            let base = tempdir.path();
            if let Ok(m) = Mount::new(self.get_device_path(), base, fs, MountFlags::empty(), None) {
                return func(Some((base, m.into_unmount_drop(UnmountFlags::DETACH))));
            }
        }

        func(None)
    }

    /// Detects if an OS is installed to this partition, and if so, what the OS
    /// is named.
    fn probe_os(&self) -> Option<OS> {
        self.get_file_system()
            .and_then(|fs| detect_os(self.get_device_path(), fs))
    }

    /// Executes a given file system's dump command to obtain the minimum shrink size
    ///
    /// Returns `io::ErrorKind::NotFound` if getting usage is not supported.
    fn sectors_used(&self) -> io::Result<u64> {
        self.get_file_system()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no file system"))
            .and_then(|fs| sectors_used(self.get_device_path(), fs))
    }
}

/// Defines whether the partition is a primary, logical, or extended partition.
///
/// # Note
///
/// This only applies for MBR partition tables.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionType {
    Primary,
    Logical,
    Extended,
}
