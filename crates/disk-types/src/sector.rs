use crate::device::{BlockDeviceExt};
use std::str::{self, FromStr};
use sysfs_class::{Block, SysClass};

/// Trait for getting and sectors from a device.
pub trait SectorExt: BlockDeviceExt {
    /// The combined total number of sectors on the disk.
    fn get_sectors(&self) -> u64;

    // fn get_sectors(&self) -> u64 {
    //     let size_file = self.sys_block_path().join("size");
    //     crate::utils::read_file::<u64>(&size_file).expect("no sector count found")
    // }

    /// The size of each logical sector, in bytes.
    fn get_logical_block_size(&self) -> u64 {
        debug!("get block size for {:?}", self.sys_block_path());

        let block = match Block::from_path(&self.sys_block_path()) {
            Ok(block) => block,
            _ => return 512
        };

        match block.queue_logical_block_size() {
            Ok(size) => return size,
            Err(_) => {
                return self.get_parent_device()
                    .expect("partition lacks parent block device")
                    .queue_logical_block_size()
                    .expect("parent of partition lacks logical block size");
            }
        }
    }

    /// The size of each logical sector, in bytes.
    fn get_physical_block_size(&self) -> u64 {
        let path = self.sys_block_path().join("queue/physical_block_size");
        crate::utils::read_file::<u64>(&path).expect("physical block size not found")
    }

    /// Calculates the requested sector from a given `Sector` variant.
    fn get_sector(&self, sector: Sector) -> u64 {
        const MIB2: u64 = 2 * 1024 * 1024;

        let end = || self.get_sectors() - (MIB2 / self.get_logical_block_size());
        let megabyte = |size| (size * 1_000_000) / self.get_logical_block_size();

        match sector {
            Sector::Start => MIB2 / self.get_logical_block_size(),
            Sector::End => end(),
            Sector::Megabyte(size) => megabyte(size),
            Sector::MegabyteFromEnd(size) => end() - megabyte(size),
            Sector::Unit(size) => size,
            Sector::UnitFromEnd(size) => end() - size,
            Sector::Percent(value) => {
                if value == ::std::u16::MAX {
                    self.get_sectors()
                } else {
                    ((self.get_sectors() * self.get_logical_block_size()) / ::std::u16::MAX as u64)
                        * value as u64
                        / self.get_logical_block_size()
                }
            }
        }
    }
}

/// Used with the `Disk::get_sector` method for converting a more human-readable unit
/// into the corresponding sector for the given disk.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum Sector {
    /// The first sector in the disk where partitions should be created.
    Start,
    /// The last sector in the disk where partitions should be created.
    End,
    /// A raw value that directly corrects to the exact number of sectors that
    /// will be used.
    Unit(u64),
    /// Similar to the above, but subtracting from the end.
    UnitFromEnd(u64),
    /// Rather than specifying the sector count, the user can specify the actual size in megabytes.
    /// This value will later be used to get the exact sector count based on the sector size.
    Megabyte(u64),
    /// Similar to the above, but subtracting from the end.
    MegabyteFromEnd(u64),
    /// The percent can be represented by specifying a value between 0 and
    /// u16::MAX, where u16::MAX is 100%.
    Percent(u16),
}

impl From<u64> for Sector {
    fn from(sectors: u64) -> Sector { Sector::Unit(sectors) }
}

impl FromStr for Sector {
    type Err = &'static str;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.ends_with('M') {
            if input.starts_with('-') {
                if let Ok(value) = input[1..input.len() - 1].parse::<u64>() {
                    return Ok(Sector::MegabyteFromEnd(value));
                }
            } else if let Ok(value) = input[..input.len() - 1].parse::<u64>() {
                return Ok(Sector::Megabyte(value));
            }
        } else if input.ends_with('%') {
            if let Ok(value) = input[..input.len() - 1].parse::<u16>() {
                if value <= 100 {
                    return Ok(Sector::Percent(value));
                }
            }
        } else if input == "start" {
            return Ok(Sector::Start);
        } else if input == "end" {
            return Ok(Sector::End);
        } else if input.starts_with('-') {
            if let Ok(value) = input[1..input.len()].parse::<u64>() {
                return Ok(Sector::UnitFromEnd(value));
            }
        } else if let Ok(value) = input[..input.len()].parse::<u64>() {
            return Ok(Sector::Unit(value));
        }

        Err("invalid sector value")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::u16;
    use std::path::Path;

    struct FictionalBlock(u64);

    impl SectorExt for FictionalBlock {}

    impl BlockDeviceExt for FictionalBlock {
        fn get_device_name(&self) -> &str { "fictional" }
        fn get_device_path(&self) -> &Path { Path::new("/dev/fictional")  }
        fn get_sectors(&self) -> u64 { self.0 }
        fn get_logical_block_size(&self) -> u64 { 512 }
    }

    #[test]
    fn sector_get() {
        let block = FictionalBlock(100_000_000);
        assert_eq!(4096, block.get_sector(Sector::Start));
        assert_eq!(99_995_904, block.get_sector(Sector::End));
        assert_eq!(1000, block.get_sector(Sector::Unit(1000)));
        assert_eq!(99_994_904, block.get_sector(Sector::UnitFromEnd(1000)));
    }

    #[test]
    fn sector_get_megabyte() {
        let block = FictionalBlock(100_000_000);
        assert_eq!(2_000_000, block.get_sector(Sector::Megabyte(1024)));
        assert_eq!(97_995_904, block.get_sector(Sector::MegabyteFromEnd(1024)));
    }

    #[test]
    fn sector_get_percent() {
        let block = FictionalBlock(100_000_000);
        assert_eq!(0, block.get_sector(Sector::Percent(0)));
        assert_eq!(24_998_826, block.get_sector(Sector::Percent(u16::MAX / 4)));
        assert_eq!(49_999_178, block.get_sector(Sector::Percent(u16::MAX / 2)));
        assert_eq!(100_000_000, block.get_sector(Sector::Percent(u16::MAX)));
    }

    #[test]
    fn sector_percentages() {
        assert_eq!("0%".parse::<Sector>(), Ok(Sector::Percent(0)));
        assert_eq!("50%".parse::<Sector>(), Ok(Sector::Percent(50)));
        assert_eq!("100%".parse::<Sector>(), Ok(Sector::Percent(100)));
    }

    #[test]
    fn sector_ends() {
        assert_eq!("start".parse::<Sector>(), Ok(Sector::Start));
        assert_eq!("end".parse::<Sector>(), Ok(Sector::End));
    }

    #[test]
    fn sector_units() {
        assert_eq!("0".parse::<Sector>(), Ok(Sector::Unit(0)));
        assert_eq!("1024".parse::<Sector>(), Ok(Sector::Unit(1024)));
        assert_eq!("-1024".parse::<Sector>(), Ok(Sector::UnitFromEnd(1024)));
    }

    #[test]
    fn sector_megabytes() {
        assert_eq!("0M".parse::<Sector>(), Ok(Sector::Megabyte(0)));
        assert_eq!("500M".parse::<Sector>(), Ok(Sector::Megabyte(500)));
        assert_eq!("20480M".parse::<Sector>(), Ok(Sector::Megabyte(20480)));
        assert_eq!("-0M".parse::<Sector>(), Ok(Sector::MegabyteFromEnd(0)));
        assert_eq!("-500M".parse::<Sector>(), Ok(Sector::MegabyteFromEnd(500)));
        assert_eq!("-20480M".parse::<Sector>(), Ok(Sector::MegabyteFromEnd(20480)));
    }
}
