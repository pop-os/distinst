use super::{DiskError, PartitionBuilder, PartitionInfo, PartitionTable, PartitionType};
use super::partitions::check_partition_size;
use std::path::Path;

pub trait DiskExt {
    fn get_table_type(&self) -> Option<PartitionTable>;

    fn get_sectors(&self) -> u64;

    fn get_sector_size(&self) -> u64;

    fn get_partitions(&self) -> &[PartitionInfo];

    fn get_partitions_mut(&mut self) -> &mut [PartitionInfo];

    fn get_device_path(&self) -> &Path;

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
