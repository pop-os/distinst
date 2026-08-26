use crate::{
    device::BlockDeviceExt,
    sector::SectorExt,
    fs::FileSystem::{self, *},
    usage::sectors_used,
};
use libparted::PartitionFlag;
use os_detect::{detect_os_from_device, OS};
use std::{io, path::Path};
use sys_mount::*;
use tempdir::TempDir;

/// Trait to provide methods for interacting with partition-based block device.
pub trait PartitionExt: BlockDeviceExt + SectorExt {
    /// Defines the file system that this device was partitioned with.
    fn get_file_system(&self) -> Option<FileSystem>;

    /// Ped partition flags that this disk has been assigned.
    fn get_partition_flags(&self) -> &[PartitionFlag];

    /// The label of the partition, if it has one.
    fn get_partition_label(&self) -> Option<&str>;

    /// Whether this partition is primary, logical, or extended.
    ///
    /// This only applies to MBR partition tables. Partitions are always `Primary` on GPT.
    fn get_partition_type(&self) -> PartitionType;

    /// The sector where this partition ends on the parent block device.
    fn get_sector_end(&self) -> u64;

    /// The sector where this partition begins on the parent block device..
    fn get_sector_start(&self) -> u64;

    /// True if the partition is an ESP partition.
    fn is_esp_partition(&self) -> bool {
        self.get_file_system().map_or(false, |fs| {
            (fs == Fat16 || fs == Fat32)
                && self.get_partition_flags().contains(&PartitionFlag::PED_PARTITION_ESP)
        })
    }

    /// True if the partition is compatible for Linux to be installed on it.
    fn is_linux_compatible(&self) -> bool {
        self.get_file_system().map_or(false, |fs| match fs {
            Exfat | Ntfs | Fat16 | Fat32 | Lvm | Luks | Swap => false,
            Btrfs | Xfs | Ext2 | Ext3 | Ext4 | F2fs => true,
        })
    }

    /// True if this is a LUKS partition
    fn is_luks(&self) -> bool { self.get_file_system().map_or(false, |fs| fs == FileSystem::Luks) }

    /// True if the partition is a swap partition.
    fn is_swap(&self) -> bool { self.get_file_system().map_or(false, |fs| fs == FileSystem::Swap) }

    /// Mount the file system at a temporary directory, and allow the caller to scan it.
    fn probe<T, F>(&self, mut func: F) -> T
    where
        F: FnMut(Option<(&Path, UnmountDrop<Mount>)>) -> T,
    {
        let mount =
            self.get_file_system().and_then(|fs| TempDir::new("distinst").ok().map(|t| (fs, t)));

        if let Some((fs, tempdir)) = mount {
            let fs = match fs {
                FileSystem::Fat16 | FileSystem::Fat32 => "vfat",
                fs => fs.into(),
            };

            // Mount the FS to the temporary directory
            let base = tempdir.path();
            if let Ok(m) = Mount::builder().fstype(fs).mount(self.get_device_path(), base) {
                return func(Some((base, m.into_unmount_drop(UnmountFlags::DETACH))));
            }
        }

        func(None)
    }

    /// Detects if an OS is installed to this partition, and if so, what the OS
    /// is named.
    fn probe_os(&self) -> Option<OS> {
        self.get_file_system().and_then(|fs| detect_os_from_device(self.get_device_path(), fs))
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
    fn sectors_overlap(&self, start: u64, end: u64) -> bool {
        let pstart = self.get_sector_start();
        let pend = self.get_sector_end();
        !((start < pstart && end < pstart) || (start > pend && end > pend))
    }

    /// Executes a given file system's dump command to obtain the minimum shrink size
    ///
    /// The return value is measured in sectors normalized to the logical sector size
    /// of the partition.
    ///
    /// Returns `io::ErrorKind::NotFound` if getting usage is not supported.
    fn sectors_used(&self) -> io::Result<u64> {
        let sector_size = self.get_logical_block_size();
        self.get_file_system()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no file system"))
            // Fetch the 512-byte sector size
            .and_then(|fs| sectors_used(self.get_device_path(), fs))
            // Then normalize it to the actual sector size
            .map(move |sectors| sectors / (sector_size / 512))
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
        end_sector:   u64,
        filesystem:   Option<FileSystem>,
        name:         Option<String>,
        part_type:    PartitionType,
        flags:        Vec<PartitionFlag>,
    }

    impl Default for Fake {
        fn default() -> Fake {
            Self {
                start_sector: 0,
                end_sector:   1,
                filesystem:   None,
                name:         None,
                part_type:    PartitionType::Primary,
                flags:        Vec::new(),
            }
        }
    }

    impl BlockDeviceExt for Fake {
        fn get_device_name(&self) -> &str { "fictional" }

        fn get_device_path(&self) -> &Path { Path::new("/dev/fictional")  }
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
        part.start_sector = 100_000;
        part.end_sector = 10_000_000;

        assert!(part.sector_lies_within(100_000));
        assert!(part.sector_lies_within(10_000_000));
        assert!(part.sector_lies_within(5_000_000));
        assert!(!part.sector_lies_within(99_999));
        assert!(!part.sector_lies_within(10_000_001));
    }

    #[test]
    fn sectors_overlap() {
        let mut part = Fake::default();
        part.start_sector = 100_000;
        part.end_sector = 10_000_000;

        assert!(!part.sectors_overlap(0, 99999));
        assert!(part.sectors_overlap(0, 100_000));
        assert!(part.sectors_overlap(0, 100_001));
        assert!(part.sectors_overlap(200_000, 1_000_000));
        assert!(part.sectors_overlap(9_999_999, 11_000_000));
        assert!(part.sectors_overlap(10_000_000, 11_000_000));
        assert!(!part.sectors_overlap(10_000_001, 11_000_000));
        assert!(part.sectors_overlap(0, 20_000_000))
    }
}
