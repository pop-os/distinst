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

/// Trait to provide methods for interacting with partition-based block device.
pub trait PartitionExt: BlockDeviceExt {
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

    /// True if the partition is an ESP partition.
    fn is_esp_partition(&self) -> bool {
        self.get_file_system().map_or(false, |fs| (fs == Fat16 || fs == Fat32)
            && self.get_partition_flags().contains(&PartitionFlag::PED_PARTITION_ESP))
    }

    /// True if the partition is compatible for Linux to be installed on it.
    fn is_linux_compatible(&self) -> bool {
        self.get_file_system()
            .map_or(false, |fs| match fs {
                Exfat | Ntfs | Fat16 | Fat32 | Lvm | Luks | Swap => false,
                Btrfs | Xfs | Ext2 | Ext3 | Ext4 | F2fs => true
            })
    }

    /// True if this is a LUKS partition
    fn is_luks(&self) -> bool {
        self.get_file_system()
            .map_or(false, |fs| fs == FileSystem::Luks)
    }

    /// True if the partition is a swap partition.
    fn is_swap(&self) -> bool {
        self.get_file_system()
            .map_or(false, |fs| fs == FileSystem::Swap)
    }

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

    /// True if the sectors in the compared partition differs from the source.
    fn sectors_differ_from<P: PartitionExt>(&self, other: &P) -> bool {
        self.get_sector_start() != other.get_sector_start()
            || self.get_sector_end() != other.get_sector_end()
    }

    /// True if the given sector lies within this partition.
    fn sector_lies_within(&self, sector: u64) -> bool {
        sector >= self.get_sector_start() && sector <= self.get_sector_end()
    }

    /// True if there is an overlap in sectors between both partitions.
    fn sectors_overlap_with(&self, start: u64, end: u64) -> bool {
        let pstart = self.get_sector_start();
        let pend = self.get_sector_end();
        !((start < pstart && end < pstart) || (start > pend && end > pend))
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        start_sector: u64,
        end_sector: u64,
        filesystem: Option<FileSystem>,
        name: Option<String>,
        part_type: PartitionType,
        flags: Vec<PartitionFlag>
    }

    impl Default for Fake {
        fn default() -> Fake {
            Self {
                start_sector: 0,
                end_sector: 1,
                filesystem: None,
                name: None,
                part_type: PartitionType::Primary,
                flags: Vec::new()
            }
        }
    }

    impl BlockDeviceExt for Fake {
        fn get_device_path(&self) -> &Path { &Path::new("/dev/fake/block") }
        fn get_mount_point(&self) -> Option<&Path> { None }
    }

    impl PartitionExt for Fake {
        fn get_file_system(&self) -> Option<FileSystem> { self.filesystem }

        fn get_partition_flags(&self) -> &[PartitionFlag] { &self.flags }

        fn get_partition_label(&self) -> Option<&str> { self.name.as_ref().map(|s| s.as_str()) }

        fn get_partition_type(&self) -> PartitionType { self.part_type }

        fn get_sector_end(&self) -> u64 { self.end_sector }

        fn get_sector_start(&self) -> u64 { self.start_sector }
    }

    #[test]
    fn sector_lies_within() {
        let mut part = Fake::default();
        part.start_sector = 100000;
        part.end_sector = 10000000;

        assert!(part.sector_lies_within(100000));
        assert!(part.sector_lies_within(10000000));
        assert!(part.sector_lies_within(5000000));
        assert!(!part.sector_lies_within(99999));
        assert!(!part.sector_lies_within(10000001));
    }

    #[test]
    fn sectors_overlap_with() {
        let mut part = Fake::default();
        part.start_sector = 100000;
        part.end_sector = 10000000;

        assert!(!part.sectors_overlap_with(0, 99999));
        assert!(part.sectors_overlap_with(0, 100000));
        assert!(part.sectors_overlap_with(0, 100001));
        assert!(part.sectors_overlap_with(200000, 1000000));
        assert!(part.sectors_overlap_with(9999999, 11000000));
        assert!(part.sectors_overlap_with(10000000, 11000000));
        assert!(!part.sectors_overlap_with(10000001, 11000000));
        assert!(part.sectors_overlap_with(0, 20000000))
    }
}
