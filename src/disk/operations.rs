use libparted::{Disk as PedDisk, Geometry, Partition as PedPartition,
                PartitionType as PedPartitionType, PartitionFlag,
                FileSystemType as PedFileSystemType};
use super::*;
use std::path::Path;
use format::mkfs;

/// Removes a partition by its ID from the disk.
fn remove_partition(disk: &mut PedDisk, partition: u32) -> Result<(), DiskError> {
    disk.remove_partition(partition)
        .map_err(|why| DiskError::PartitionRemove {
            partition: partition as i32,
            why,
        })
}

/// Obtains a partition from the disk by its ID.
fn get_partition<'a>(disk: &'a mut PedDisk, part: u32) -> Result<PedPartition<'a>, DiskError> {
    disk.get_partition(part)
        .ok_or(DiskError::PartitionNotFound {
            partition: part as i32,
        })
}

/// The first state of disk operations, which provides a method for removing partitions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DiskOps<'a> {
    pub(crate) device_path: &'a Path,
    pub(crate) remove_partitions: Vec<i32>,
    pub(crate) change_partitions: Vec<PartitionChange>,
    pub(crate) create_partitions: Vec<PartitionCreate>,
}

impl<'a> DiskOps<'a> {
    pub(crate) fn remove(self) -> Result<ChangePartitions<'a>, DiskError> {
        let mut device = open_device(self.device_path)?;

        {
            let mut disk = open_disk(&mut device)?;
            let mut changes_required = false;
            for partition in self.remove_partitions {
                info!(
                    "adding partition {} from {} for removal",
                    partition,
                    self.device_path.display()
                );
                remove_partition(&mut disk, partition as u32)?;
                changes_required = true;
            }

            if changes_required {
                info!(
                    "attempting to remove partitions from {}",
                    self.device_path.display()
                );
                commit(&mut disk)?;
                info!(
                    "successfully removed partitions from {}",
                    self.device_path.display()
                );
            }
        }

        sync(&mut device)?;
        Ok(ChangePartitions {
            device_path: self.device_path,
            change_partitions: self.change_partitions,
            create_partitions: self.create_partitions,
        })
    }
}

/// The second state of disk operations, which provides a method for changing partitions.
pub(crate) struct ChangePartitions<'a> {
    device_path: &'a Path,
    change_partitions: Vec<PartitionChange>,
    create_partitions: Vec<PartitionCreate>,
}

impl<'a> ChangePartitions<'a> {
    pub(crate) fn change(self) -> Result<CreatePartitions<'a>, DiskError> {
        let mut device = open_device(self.device_path)?;

        for change in &self.change_partitions {
            let mut disk = open_disk(&mut device)?;
            let mut resize_required = false;
            let mut flags_changed = false;

            {
                // Obtain the partition that needs to be changed by its ID.
                let mut part = get_partition(&mut disk, change.num as u32)?;

                for flag in &change.flags {
                    if part.is_flag_available(*flag) {
                        match part.set_flag(*flag, true) {
                            Ok(()) => flags_changed = true,
                            Err(_) => {
                                info!(
                                    "unable to set {:?} for {}{}",
                                    flag,
                                    self.device_path.display(),
                                    change.num
                                );
                            }
                        }
                    }
                }

                // For convenience, grab the start and env sectors from the partition's geom.
                let (start, end) = (part.geom_start() as u64, part.geom_end() as u64);

                // If the partition needs to be resized/moved, this will execute.
                if end != change.end || start != change.start {
                    resize_required = true;

                    // Grab the geometry, duplicate it, set the new values, and open the FS.
                    let mut geom = part.get_geom();
                    let mut new_geom = geom.duplicate().map_err(|_| DiskError::GeometryDuplicate)?;

                    // libparted will automatically set the length after manually setting the
                    // start and end sector values.
                    new_geom
                        .set_start(change.start as i64)
                        .and_then(|_| new_geom.set_end(change.end as i64))
                        .map_err(|_| DiskError::GeometrySet)?;

                    // Open the FS located at the original geometry coordinates.
                    let mut fs = geom.open_fs().ok_or(DiskError::NoFilesystem)?;

                    // Resize the file system with the new geometry's data.
                    info!(
                        "will partition {} on {} from {}:{} to {}:{}",
                        change.num,
                        self.device_path.display(),
                        start,
                        end,
                        change.start,
                        change.end,
                    );
                    fs.resize(&new_geom, None)
                        .map_err(|_| DiskError::PartitionResize)?;
                }
            }

            if resize_required || flags_changed {
                // Commit all the partition move/resizing operations.
                if resize_required {
                    info!("resizing {} on {}", change.num, self.device_path.display());
                }

                commit(&mut disk)?;

                if resize_required {
                    info!(
                        "successfully resized {} on {}",
                        change.num,
                        self.device_path.display()
                    );
                }
            }
        }
        // Flush the OS cache and drop the device before proceeding to formatting.
        sync(&mut device)?;
        drop(device);

        // Format all the partitions that need to be formatted.
        for change in &self.change_partitions {
            let device_path = format!("{}{}", self.device_path.display(), change.num);
            if let Some(fs) = change.format {
                mkfs(&device_path, fs).map_err(|why| DiskError::PartitionFormat { why })?;
            }
        }

        // Proceed to the next state in the machine.
        Ok(CreatePartitions {
            device_path: self.device_path,
            create_partitions: self.create_partitions,
        })
    }
}

/// The final state of disk operations, which provides a method for creating new partitions.
pub(crate) struct CreatePartitions<'a> {
    device_path: &'a Path,
    create_partitions: Vec<PartitionCreate>,
}

impl<'a> CreatePartitions<'a> {
    /// If any new partitions were specified, they will be created here.
    pub(crate) fn create(self) -> Result<(), DiskError> {
        for partition in &self.create_partitions {
            let mut device = open_device(self.device_path)?;
            {
                // Create a new geometry from the start sector and length of the new partition.
                let length = partition.end_sector - partition.start_sector;
                let geometry = Geometry::new(&device, partition.start_sector as i64, length as i64)
                    .map_err(|why| DiskError::GeometryCreate { why })?;

                // Convert our internal partition type enum into libparted's variant.
                let part_type = match partition.kind {
                    PartitionType::Primary => PedPartitionType::PED_PARTITION_NORMAL,
                    PartitionType::Logical => PedPartitionType::PED_PARTITION_LOGICAL,
                };

                // Open the disk, create the new partition, and add it to the disk.
                let (start, end) = (geometry.start(), geometry.start() + geometry.length());

                let fs_type = PedFileSystemType::get(partition.file_system.into()).unwrap();

                let mut disk = open_disk(&mut device)?;
                let mut part = PedPartition::new(&mut disk, part_type, Some(&fs_type), start, end)
                    .map_err(|why| DiskError::PartitionCreate { why })?;

                for &flag in &partition.flags {
                    if part.is_flag_available(flag) {
                        if let Err(_) = part.set_flag(flag, true) {
                            info!("unable to set {:?}", flag);
                        }
                    }
                }

                // Add the partition, and commit the changes to the disk.
                let constraint = geometry.exact().unwrap();
                disk.add_partition(&mut part, &constraint)
                    .map_err(|why| DiskError::PartitionCreate { why })?;

                // Attempt to write the new partition to the disk.
                info!(
                    "creating new partition ({}:{}) on {}",
                    start,
                    end,
                    self.device_path.display()
                );
                commit(&mut disk)?;
                info!("successfully created new partition");
            }

            // Flush the OS caches before proceeding to format the new partition.
            sync(&mut device)?;
            drop(device);

            // Open a second instance of the disk which we need to get the new partition ID.
            let mut device = open_device(self.device_path)?;
            let disk = open_disk(&mut device)?;
            let num = disk.get_partition_by_sector(partition.start_sector as i64)
                .map(|part| part.num())
                .ok_or(DiskError::NewPartNotFound)?;

            // Finally, partition the newly-created partition.
            let path = format!("{}{}", self.device_path.display(), num);
            mkfs(&path, partition.file_system).map_err(|why| DiskError::PartitionFormat { why })?;
        }

        Ok(())
    }
}

/// Defines the move and resize operations that the partition with this number
/// will need to perform.
///
/// If the `start` sector differs from the source, the partition will be moved.
/// If the `end` minus the `start` differs from the length of the source, the
/// partition will be resized. Once partitions have been moved and resized,
/// they will be formatted accordingly, if formatting was set.
///
/// # Note
///
/// Resize operations should always be performed before move operations.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartitionChange {
    /// The partition ID that will be changed.
    pub(crate) num: i32,
    /// The start sector that the partition will have.
    pub(crate) start: u64,
    /// The end sector that the partition will have.
    pub(crate) end: u64,
    /// Whether the partition should be reformatted, and if so, to what format.
    pub(crate) format: Option<FileSystemType>,
    /// Flags which should be set on the partition.
    pub(crate) flags: Vec<PartitionFlag>,
}

/// Defines a new partition to be created on the file system.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartitionCreate {
    /// The start sector that the partition will have.
    pub(crate) start_sector: u64,
    /// The end sector that the partition will have.
    pub(crate) end_sector: u64,
    /// The format that the file system should be formatted to.
    pub(crate) file_system: FileSystemType,
    /// Whether the partition should be primary or logical.
    pub(crate) kind: PartitionType,
    /// Flags which should be set on the partition.
    pub(crate) flags: Vec<PartitionFlag>,
}
