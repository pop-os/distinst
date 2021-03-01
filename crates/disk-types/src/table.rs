use crate::{device::BlockDeviceExt, partition::PartitionType};

/// Specifies whether the partition table on the disk is **MSDOS** or **GPT**.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionTable {
    Msdos,
    Gpt,
}

/// A possible error when validating the partition table.
#[derive(Debug, Error, PartialEq)]
pub enum PartitionTableError {
    #[error(display = "primary partitions exceeded on partition table")]
    PrimaryPartitionsExceeded,
    #[error(display = "partition table not found")]
    NotFound,
}

/// Methods for block devices that may have a partition table.
pub trait PartitionTableExt: BlockDeviceExt {
    /// Fetch the partition table info on this device, if it exists.
    fn get_partition_table(&self) -> Option<PartitionTable>;

    /// Obtain the number of primary and logical partitions, in that order.
    fn get_partition_type_count(&self) -> (usize, usize, bool);

    /// Checks if the additional partition type can be added to the partition table.
    fn supports_additional_partition_type(
        &self,
        new_type: PartitionType,
    ) -> Result<(), PartitionTableError> {
        match self.get_partition_table() {
            Some(PartitionTable::Gpt) => (),
            Some(PartitionTable::Msdos) => {
                let (primary, logical, extended) = self.get_partition_type_count();
                if new_type == PartitionType::Primary {
                    if primary >= 4 || (primary >= 3 && (extended || logical != 0)) {
                        return Err(PartitionTableError::PrimaryPartitionsExceeded);
                    }
                } else if primary >= 4 {
                    return Err(PartitionTableError::PrimaryPartitionsExceeded);
                }
            }
            None => return Err(PartitionTableError::NotFound),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    pub struct FictionalBlock {
        partitions: Vec<PartitionType>,
    }

    impl BlockDeviceExt for FictionalBlock {
        fn get_device_name(&self) -> &str { "fictional" }

        fn get_device_path(&self) -> &Path { Path::new("/dev/fictional")  }

        fn get_mount_point(&self) -> Option<&Path> { None }
    }

    impl PartitionTableExt for FictionalBlock {
        fn get_partition_table(&self) -> Option<PartitionTable> { Some(PartitionTable::Msdos) }

        fn get_partition_type_count(&self) -> (usize, usize, bool) {
            self.partitions.iter().fold((0, 0, false), |sum, &part| match part {
                PartitionType::Logical => (sum.0, sum.1 + 1, sum.2),
                PartitionType::Primary => (sum.0 + 1, sum.1, sum.2),
                PartitionType::Extended => (sum.0, sum.1, true),
            })
        }
    }

    #[test]
    fn partition_table_msdos_checks() {
        let maxed_block = FictionalBlock {
            partitions: vec![
                PartitionType::Primary,
                PartitionType::Primary,
                PartitionType::Primary,
                PartitionType::Primary,
            ],
        };

        assert_eq!(
            maxed_block.supports_additional_partition_type(PartitionType::Primary),
            Err(PartitionTableError::PrimaryPartitionsExceeded)
        );

        assert_eq!(
            maxed_block.supports_additional_partition_type(PartitionType::Logical),
            Err(PartitionTableError::PrimaryPartitionsExceeded)
        );

        let max_extended = FictionalBlock {
            partitions: vec![
                PartitionType::Primary,
                PartitionType::Primary,
                PartitionType::Primary,
                PartitionType::Extended,
                PartitionType::Logical,
                PartitionType::Logical,
            ],
        };

        assert_eq!(
            max_extended.supports_additional_partition_type(PartitionType::Primary),
            Err(PartitionTableError::PrimaryPartitionsExceeded)
        );

        assert_eq!(max_extended.supports_additional_partition_type(PartitionType::Logical), Ok(()));

        let free = FictionalBlock {
            partitions: vec![
                PartitionType::Primary,
                PartitionType::Primary,
                PartitionType::Extended,
                PartitionType::Logical,
                PartitionType::Logical,
            ],
        };

        assert_eq!(free.supports_additional_partition_type(PartitionType::Primary), Ok(()));

        assert_eq!(free.supports_additional_partition_type(PartitionType::Logical), Ok(()));
    }
}
