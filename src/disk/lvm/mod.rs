mod detect;
mod encryption;

pub(crate) use self::detect::physical_volumes_to_deactivate;
pub use self::encryption::LvmEncryption;
use super::{DiskError, DiskExt, PartitionInfo, PartitionTable, PartitionType};
use super::external::{lvcreate, mkfs, vgcreate};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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

    pub fn encrypt_with_password(&mut self, physical_volume: &str, password: &str) {
        // TODO: NLL
        if self.encryption.is_some() {
            if let Some(ref mut encryption) = self.encryption.as_mut() {
                encryption.password = Some(password.into());
            }
        } else {
            self.encryption = Some(LvmEncryption {
                physical_volume: physical_volume.into(),
                password:        Some(password.into()),
                keydata:         None,
            });
        }
    }

    /// LVM Partitions should have names!
    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub(crate) fn validate(&self) -> Result<(), DiskError> {
        // TODO: Requires Rust 1.23
        // self.partitions.iter()
        //     .map(|p| p.name.as_ref().ok_or_else(|_| DiskError::VolumePartitionLacksLabel))
        //     .collect()

        for partition in self.get_partitions() {
            if !partition.name.is_some() {
                return Err(DiskError::VolumePartitionLacksLabel);
            }
        }

        Ok(())
    }

    pub(crate) fn create_volume_group<I, S>(&self, blocks: I) -> Result<(), DiskError>
    where
        I: Iterator<Item = S>,
        S: AsRef<OsStr>,
    {
        vgcreate(&self.volume_group, blocks).map_err(|why| DiskError::VolumeGroupCreate { why })
    }

    pub(crate) fn create_partitions(&self) -> Result<(), DiskError> {
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
