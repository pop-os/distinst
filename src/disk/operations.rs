use super::*;
use std::path::Path;

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
        for part_id in self.remove_partitions.into_iter() {}

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
