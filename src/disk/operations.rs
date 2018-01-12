use super::*;

/// The first state of disk operations, which provides a method for removing partitions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DiskOps {
    pub(crate) remove_partitions: Vec<i32>,
    pub(crate) change_partitions: Vec<PartitionChange>,
    pub(crate) create_partitions: Vec<PartitionCreate>,
}

impl DiskOps {
    pub(crate) fn remove(self) -> Result<ChangePartitions, DiskError> {
        Ok(ChangePartitions {
            change_partitions: self.change_partitions,
            create_partitions: self.create_partitions,
        })
    }
}

/// The second state of disk operations, which provides a method for changing partitions.
pub(crate) struct ChangePartitions {
    change_partitions: Vec<PartitionChange>,
    create_partitions: Vec<PartitionCreate>,
}

impl ChangePartitions {
    pub(crate) fn change(self) -> Result<CreatePartitions, DiskError> {
        Ok(CreatePartitions {
            create_partitions: self.create_partitions,
        })
    }
}

/// The final state of disk operations, which provides a method for creating new partitions.
pub(crate) struct CreatePartitions {
    create_partitions: Vec<PartitionCreate>,
}

impl CreatePartitions {
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
