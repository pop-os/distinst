use super::{DiskError, PartitionBuilder, PartitionInfo, PartitionTable, PartitionType};
use super::partitions::check_partition_size;
use std::path::Path;
use std::str::{self, FromStr};

/// Used with the `Disk::get_sector` method for converting a more human-readable unit
/// into the corresponding sector for the given disk.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum Sector {
    /// The first sector in the disk where partitions should be created.
    Start,
    /// The last sector in the disk where partitions should be created.
    End,
    /// A raw value that directly corrects to the exact number of sectors that will be used.
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

// TODO: Write tests for this.

impl FromStr for Sector {
    type Err = &'static str;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.ends_with("M") {
            if input.starts_with("-") {
                if let Ok(value) = input[1..input.len() - 1].parse::<u64>() {
                    return Ok(Sector::MegabyteFromEnd(value));
                }
            } else if let Ok(value) = input[..input.len() - 1].parse::<u64>() {
                return Ok(Sector::Megabyte(value));
            }
        } else if input.ends_with("%") {
            if let Ok(value) = input[..input.len() - 1].parse::<u16>() {
                if value <= 100 {
                    return Ok(Sector::Percent(value));
                }
            }
        } else if input == "start" {
            return Ok(Sector::Start);
        } else if input == "end" {
            return Ok(Sector::End);
        } else if input.starts_with("-") {
            if let Ok(value) = input[1..input.len()].parse::<u64>() {
                return Ok(Sector::UnitFromEnd(value));
            }
        } else if let Ok(value) = input[..input.len()].parse::<u64>() {
            return Ok(Sector::Unit(value));
        }

        Err("invalid sector value")
    }
}

/// Contains methods that are shared between physical and logical disk devices.
pub trait DiskExt {
    /// The partition table that is on the device.
    fn get_table_type(&self) -> Option<PartitionTable>;

    /// The combined total number of sectors on the disk.
    fn get_sectors(&self) -> u64;

    /// The size of each sector, in bytes.
    fn get_sector_size(&self) -> u64;

    /// Returns a slice of all partitions in the device.
    fn get_partitions(&self) -> &[PartitionInfo];

    /// Returns a mutable slice of all partitions in the device.
    fn get_partitions_mut(&mut self) -> &mut [PartitionInfo];

    /// Returns the path to the block device in the system.
    fn get_device_path(&self) -> &Path;

    /// Validates that the partitions are valid for the partition table
    fn validate_partition_table(&self, part_type: PartitionType) -> Result<(), DiskError>;

    /// If a given start and end range overlaps a pre-existing partition, that
    /// partition's number will be returned to indicate a potential conflict.
    fn overlaps_region(&self, start: u64, end: u64) -> Option<i32> {
        self.get_partitions().iter()
            // Only consider partitions which are not set to be removed.
            .filter(|part| !part.remove)
            // Return upon the first partition where the sector is within the partition.
            .find(|part|
                !(
                    (start < part.start_sector && end < part.start_sector)
                    || (start > part.end_sector && end > part.end_sector)
                )
            )
            // If found, return the partition number.
            .map(|part| part.number)
    }

    #[allow(cast_lossless)]
    /// Calculates the requested sector from a given `Sector` variant.
    fn get_sector(&self, sector: Sector) -> u64 {
        const MIB2: u64 = 2 * 1024 * 1024;

        let end = || self.get_sectors() - (MIB2 / self.get_sector_size());
        let megabyte = |size| (size * 1_000_000) / self.get_sector_size();

        match sector {
            Sector::Start => MIB2 / self.get_sector_size(),
            Sector::End => end(),
            Sector::Megabyte(size) => megabyte(size),
            Sector::MegabyteFromEnd(size) => end() - megabyte(size),
            Sector::Unit(size) => size,
            Sector::UnitFromEnd(size) => end() - size,
            Sector::Percent(value) => {
                ((self.get_sectors() * self.get_sector_size()) / ::std::u16::MAX as u64)
                    * value as u64 / self.get_sector_size()
            }
        }
    }

    /// Adds a new partition to the partition list.
    fn push_partition(&mut self, partition: PartitionInfo);

    /// Adds a partition to the partition scheme.
    ///
    /// An error can occur if the partition will not fit onto the disk.
    fn add_partition(&mut self, builder: PartitionBuilder) -> Result<(), DiskError> {
        info!(
            "libdistinst: checking if {}:{} overlaps",
            builder.start_sector, builder.end_sector
        );

        // Ensure that the values aren't already contained within an existing partition.
        if let Some(id) = self.overlaps_region(builder.start_sector, builder.end_sector) {
            return Err(DiskError::SectorOverlaps { id });
        }

        // And that the end can fit onto the disk.
        if self.get_sectors() < builder.end_sector as u64 {
            return Err(DiskError::PartitionOOB);
        }

        // Perform partition table & MSDOS restriction tests.
        self.validate_partition_table(builder.part_type)?;

        let fs = builder.filesystem.clone();
        let partition = builder.build();
        check_partition_size(partition.sectors() * self.get_sector_size(), fs)?;
        self.push_partition(partition);

        Ok(())
    }
}

/// Finds the partition block path and associated partition information that is associated with
/// the given target mount point.
pub(crate) fn find_partition<'a, T: DiskExt>(
    disks: &'a [T],
    target: &Path,
) -> Option<(&'a Path, &'a PartitionInfo)> {
    for disk in disks {
        for partition in disk.get_partitions() {
            if let Some(ref ptarget) = partition.target {
                if ptarget == target {
                    return Some((disk.get_device_path(), partition));
                }
            }
        }
    }
    None
}
