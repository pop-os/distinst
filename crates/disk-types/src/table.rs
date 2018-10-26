use device::BlockDeviceExt;
use partition::PartitionType;

/// Specifies whether the partition table on the disk is **MSDOS** or **GPT**.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionTable {
    Msdos,
    Gpt,
}

#[derive(Debug, Fail)]
pub enum PartitionTableError {
    #[fail(display = "primary partitions exceeded on partition table")]
    PrimaryPartitionsExceeded,
    #[fail(display = "partition table not found")]
    NotFound,
}

pub trait PartitionTableExt: BlockDeviceExt {
    /// Fetch the partition table info on this device, if it exists.
    fn get_partition_table(&self) -> Option<PartitionTable>;

    /// Obtain the number of primary and logical partitions, in that order.
    fn get_partition_type_count(&self) -> (usize, usize);

    fn supports_additional_partition_type(&self, new_type: PartitionType) -> Result<(), PartitionTableError> {
        match self.get_partition_table() {
            Some(PartitionTable::Gpt) => (),
            Some(PartitionTable::Msdos) => {
                let (primary, logical) = self.get_partition_type_count();
                if new_type == PartitionType::Primary {
                    if primary == 4 || (primary == 3 && logical != 0) {
                        return Err(PartitionTableError::PrimaryPartitionsExceeded);
                    }
                } else if primary == 4 {
                    return Err(PartitionTableError::PrimaryPartitionsExceeded);
                }
            }
            None => return Err(PartitionTableError::NotFound),
        }

        Ok(())
    }
}
