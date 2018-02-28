mod detect;
mod encryption;

pub(crate) use self::detect::physical_volumes_to_deactivate;
pub use self::encryption::LvmEncryption;
use super::{DiskError, DiskExt, PartitionInfo, PartitionTable, PartitionType};
use super::external::{lvcreate, mkfs, vgcreate};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// An LVM device acts similar to a Disk, but consists of one more block devices
/// that comprise a volume group, and may optionally be encrypted.
#[derive(Debug, Clone, PartialEq)]
pub struct LvmDevice {
    pub(crate) volume_group: String,
    pub(crate) device_path:  PathBuf,
    pub(crate) sectors:      u64,
    pub(crate) sector_size:  u64,
    pub(crate) partitions:   Vec<PartitionInfo>,
    pub(crate) encryption:   Option<LvmEncryption>,
}

impl DiskExt for LvmDevice {
    fn get_table_type(&self) -> Option<PartitionTable> { None }

    fn get_sectors(&self) -> u64 { self.sectors }

    fn get_sector_size(&self) -> u64 { self.sector_size }

    fn get_partitions(&self) -> &[PartitionInfo] { &self.partitions }

    fn get_partitions_mut(&mut self) -> &mut [PartitionInfo] { &mut self.partitions }

    fn get_device_path(&self) -> &Path { &self.device_path }

    fn validate_partition_table(&self, _part_type: PartitionType) -> Result<(), DiskError> {
        Ok(())
    }

    fn push_partition(&mut self, partition: PartitionInfo) { self.partitions.push(partition); }
}

impl LvmDevice {
    /// Creates a new volume group, with an optional encryption configuration.
    pub(crate) fn new(
        volume_group: String,
        encryption: Option<LvmEncryption>,
        sectors: u64,
        sector_size: u64,
    ) -> LvmDevice {
        let device_path = PathBuf::from(format!("/dev/mapper/{}", volume_group));
        LvmDevice {
            volume_group,
            device_path,
            sectors,
            sector_size,
            partitions: Vec::new(),
            encryption,
        }
    }

    pub(crate) fn add_sectors(&mut self, sectors: u64) { self.sectors += sectors; }

    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub(crate) fn validate(&self) -> Result<(), DiskError> {
        for partition in self.get_partitions() {
            if !partition.name.is_some() {
                return Err(DiskError::VolumePartitionLacksLabel);
            }
        }

        Ok(())
    }

    /// Creates the volume group using all of the supplied block devices as members of the
    /// group.
    pub(crate) fn create_volume_group<I, S>(&self, blocks: I) -> Result<(), DiskError>
    where
        I: Iterator<Item = S>,
        S: AsRef<OsStr>,
    {
        vgcreate(&self.volume_group, blocks).map_err(|why| DiskError::VolumeGroupCreate { why })
    }

    /// Create all logical volumes on the volume group, and format them.
    pub(crate) fn create_partitions(&self) -> Result<(), DiskError> {
        if self.partitions.is_empty() {
            return Ok(());
        }
        let nparts = self.partitions.len() - 1;
        for (id, partition) in self.partitions.iter().enumerate() {
            let label = partition.name.as_ref().unwrap();

            // Create the new logical volume on the volume group.
            lvcreate(
                &self.volume_group,
                &label,
                if id == nparts {
                    None
                } else {
                    Some(partition.sectors() * self.sector_size)
                },
            ).map_err(|why| DiskError::LogicalVolumeCreate { why })?;

            // Then format the newly-created logical volume
            if let Some(fs) = partition.filesystem.as_ref() {
                mkfs(&partition.device_path, fs.clone())
                    .map_err(|why| DiskError::PartitionFormat { why })?;
            }
        }

        Ok(())
    }
}
