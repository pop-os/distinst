use misc;
use super::super::{
    DiskError, Disks, PartitionBuilder, PartitionInfo,
    PartitionTable, PartitionType, Sector,
};
use super::partitions::REMOVE;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Contains methods that are shared between physical and logical disk devices.
pub trait DiskExt {
    const LOGICAL: bool;

    /// Returns true if an extended partition exists.
    fn extended_exists(&self) -> bool {
        self.get_partitions().iter().any(|p| p.part_type == PartitionType::Extended)
    }

    /// Returns the path to the block device in the system.
    fn get_device_path(&self) -> &Path;

    /// Sometimes, disks may have an entire file system, rather than a partition table.
    fn get_file_system(&self) -> Option<&PartitionInfo>;

    /// Mutable variant of `get_file_system()`.
    fn get_file_system_mut(&mut self) -> Option<&mut PartitionInfo>;

    /// Sets a file system on this device (unsetting the partition table in the process).
    fn set_file_system(&mut self, fs: PartitionInfo);

    /// Returns the model of the device.
    fn get_model(&self) -> &str;

    /// If the disk is mounted somewhere, get the mount point.
    fn get_mount_point(&self) -> Option<&Path>;

    /// Get the first partition whose start sector is after the given sector.
    fn get_partition_after(&self, sector: u64) -> Option<&PartitionInfo> {
        self.get_partitions().iter().find(|p| p.start_sector > sector)
    }

    /// Returns a slice of all partitions in the device.
    fn get_partitions(&self) -> &[PartitionInfo];

    /// Returns a mutable slice of all partitions in the device.
    fn get_partitions_mut(&mut self) -> &mut [PartitionInfo];

    /// The combined total number of sectors on the disk.
    fn get_sectors(&self) -> u64;

    /// The size of each sector, in bytes.
    fn get_sector_size(&self) -> u64;

    /// The partition table that is on the device.
    fn get_table_type(&self) -> Option<PartitionTable>;

    /// Returns true if this partition is mounted at root.
    fn contains_mount(&self, mount: &str, parent: &Disks) -> bool {
        let check_partitions = || {
            self.get_partitions().iter().any(|partition| {
                if partition.mount_point == Some(mount.into()) {
                    return true;
                }

                partition
                    .volume_group
                    .as_ref()
                    .map_or(false, |&(ref vg, _)| {
                        parent.get_logical_device(vg)
                            .map_or(false, |d| d.contains_mount(mount, parent))
                    })
            })
        };

        self.get_mount_point()
            .map_or_else(check_partitions, |m| m == Path::new(mount))
    }

    fn is_logical(&self) -> bool { Self::LOGICAL }

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

        misc::open(path.join("removable"))
            .ok()
            .and_then(|file| file.bytes().next())
            .map_or(false, |res| res.ok().map_or(false, |byte| byte == b'1'))
    }

    /// Validates that the partitions are valid for the partition table
    fn validate_partition_table(&self, part_type: PartitionType) -> Result<(), DiskError>;

    /// If a given start and end range overlaps a pre-existing partition, that
    /// partition's number will be returned to indicate a potential conflict.
    fn overlaps_region(&self, start: u64, end: u64) -> Option<i32> {
        self.get_partitions().iter()
            // Only consider partitions which are not set to be removed.
            .filter(|part| !part.flag_is_enabled(REMOVE))
            // And which aren't extended
            .filter(|part| part.part_type != PartitionType::Extended)
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

    fn get_used(&self) -> u64 {
        self.get_partitions()
            .iter()
            .filter(|p| !p.flag_is_enabled(REMOVE))
            .map(|p| p.sectors())
            .sum()
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
    fn add_partition(&mut self, mut builder: PartitionBuilder) -> Result<(), DiskError> {
        // Ensure that the values aren't already contained within an existing partition.
        if !Self::LOGICAL && builder.part_type != PartitionType::Extended {
            info!(
                "checking if {}:{} overlaps",
                builder.start_sector, builder.end_sector
            );

            if let Some(id) = self.overlaps_region(builder.start_sector, builder.end_sector) {
                return Err(DiskError::SectorOverlaps { id });
            }
        }

        // And that the end can fit onto the disk.
        if Self::LOGICAL {
            let sectors = self.get_sectors();
            let estimated_size = self.get_used() + (builder.end_sector - builder.start_sector);
            if sectors < estimated_size {
                return Err(DiskError::PartitionOOB);
            }
        } else if self.get_sectors() < builder.end_sector {
            return Err(DiskError::PartitionOOB);
        }

        // Perform partition table & MSDOS restriction tests.
        match self.validate_partition_table(builder.part_type) {
            Err(DiskError::PrimaryPartitionsExceeded) => {
                info!("primary partitions exceeded, resolving");
                builder.part_type = PartitionType::Logical;
            }
            Ok(()) => (),
            error @ Err(_) => error?,
        };

        if builder.part_type == PartitionType::Logical && !self.extended_exists() {
            info!("adding extended partition");
            let part = PartitionBuilder::new(
                builder.start_sector,
                self.get_partition_after(builder.start_sector)
                    .map_or_else(
                        || self.get_sector(Sector::End),
                        |part| part.start_sector - 1
                    ),
                None
            ).partition_type(PartitionType::Extended);

            self.push_partition(part.build());
            builder.start_sector += 1_024_000 / 512 + 1;
        }

        let fs = builder.filesystem;
        let partition = builder.build();
        if let Some(fs) = fs {
            fs.validate_size(partition.sectors() * self.get_sector_size())
                .map_err(|why| DiskError::new_partition_error(partition.device_path.clone(), why))?;
        }

        self.push_partition(partition);

        Ok(())
    }
}

/// Finds the partition block path and associated partition information that is associated with
/// the given target mount point.
pub fn find_partition<'a, T: DiskExt>(
    disks: &'a [T],
    target: &Path,
) -> Option<(&'a Path, &'a PartitionInfo)> {
    for disk in disks {
        for partition in disk.get_file_system().into_iter().chain(disk.get_partitions().iter()) {
            if let Some(ref ptarget) = partition.target {
                if ptarget == target {
                    return Some((disk.get_device_path(), partition));
                }
            }
        }
    }
    None
}

/// Finds the partition block path and associated partition information that is associated with
/// the given target mount point. Mutable variant
pub fn find_partition_mut<'a, T: DiskExt>(
    disks: &'a mut [T],
    target: &Path,
) -> Option<(PathBuf, &'a mut PartitionInfo)> {
    for disk in disks {
        let path = disk.get_device_path().to_path_buf();
        // TODO: NLL
        let disk = disk as *mut T;

        if let Some(partition) = unsafe { &mut *disk }.get_file_system_mut() {
            // TODO: NLL
            let mut found = false;
            if let Some(ref ptarget) = partition.target {
                if ptarget == target {
                    found = true;
                }
            }

            if found {
                return Some((path, partition));
            }
        }

        for partition in unsafe { &mut *disk }.get_partitions_mut() {
            // TODO: NLL
            let mut found = false;
            if let Some(ref ptarget) = partition.target {
                if ptarget == target {
                    found = true;
                }
            }

            if found {
                return Some((path, partition));
            }
        }
    }
    None
}
