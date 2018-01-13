use libparted::{Device as PedDevice, Disk as PedDisk};
use super::*;
use std::path::Path;
use ::format::mkfs;


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
        // Open the disk so that we can perform destructive changes on it.
        let mut device = PedDevice::new(self.device_path)
            .map_err(|_| DiskError::DeviceGet)?;

        {
            // Open the disk to prepare for altering it's partition scheme.
            let mut disk = PedDisk::new(&mut device)
                .map_err(|_| DiskError::DiskNew)?;

            // Delete all of the specified partitions.
            for partition in self.remove_partitions {
                disk.remove_partition(partition as u32)
                    .map_err(|err| DiskError::PartitionRemove { partition })?;
            }

            // Write the changes to the disk, and notify the OS.
            disk.commit().map_err(|_| DiskError::DiskCommit)?;
        }

        // Flush the OS cache to ensure that the OS knows about the changes.
        device.sync().map_err(|_| DiskError::DiskSync)?;

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
        let mut device = PedDevice::new(self.device_path)
            .map_err(|_| DiskError::DeviceGet)?;
        
        {
            for change in &self.change_partitions {
                let mut disk = PedDisk::new(&mut device)
                    .map_err(|_| DiskError::DiskNew)?;

                {
                    let mut part = disk.get_partition(change.num as u32)
                        .ok_or(DiskError::PartitionNotFound { partition: change.num })?;

                    let start = part.geom_start() as u64;
                    let end = part.geom_end() as u64;
                    if end != change.end || start != change.start {
                        let mut geom = part.get_geom();
                        let mut new_geom = geom.duplicate()
                            .map_err(|_| DiskError::GeometryDuplicate)?;
                        
                        new_geom.set_start(change.start as i64)
                            .and_then(|_| new_geom.set_end(change.end as i64))
                            .map_err(|_| DiskError::GeometrySet)?;

                        let mut fs = geom.open_fs().ok_or(DiskError::NoFilesystem)?;
                        fs.resize(&new_geom, None).map_err(|_| DiskError::PartitionResize)?;
                    }
                }

                disk.commit().map_err(|_| DiskError::DiskCommit)?;
            }
        }

        device.sync().map_err(|_| DiskError::DiskSync)?;
        drop(device);

        for change in &self.change_partitions {
            let partition = format!("{}{}", self.device_path.display(), change.num);
            if let Some(fs) = change.format {
                mkfs(&partition, fs).map_err(|why| DiskError::PartitionFormat { why })?;
            }
        }

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
}
