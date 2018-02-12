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

    fn get_device_path(&self) -> &Path { &self.device_path }

    fn validate_partition_table(&self, _part_type: PartitionType) -> Result<(), DiskError> {
        Ok(())
    }

    fn push_partition(&mut self, partition: PartitionInfo) { self.partitions.push(partition); }
}

impl LvmDevice {
    pub(crate) fn new(volume_group: String, sectors: u64, sector_size: u64) -> LvmDevice {
        let device_path = PathBuf::from(format!("/dev/mapper/{}", volume_group));
        LvmDevice {
            volume_group,
            device_path,
            sectors,
            sector_size,
            partitions: Vec::new(),
            encryption: None,
        }
    }

    pub(crate) fn add_sectors(&mut self, sectors: u64) { self.sectors += sectors; }

    pub fn encrypt_with_password(&mut self, password: &str) {
        // TODO: NLL
        if self.encryption.is_some() {
            if let Some(ref mut encryption) = self.encryption.as_mut() {
                encryption.password = Some(password.into());
            }
        } else {
            self.encryption = Some(LvmEncryption {
                password: Some(password.into()),
                keyfile:  None,
            });
        }
    }

    /// LVM Partitions should have names!
    pub(crate) fn validate(&self) -> Result<(), DiskError> {
        for partition in &self.partitions {
            if !partition.name.is_some() {
                return Err(DiskError::VolumePartitionLacksLabel);
            }
        }

        Ok(())
    }

    pub(crate) fn create_group<I, S>(&self, blocks: I) -> Result<(), DiskError>
    where
        I: Iterator<Item = S>,
        S: AsRef<OsStr>,
    {
        vgcreate(&self.volume_group, blocks).map_err(|why| DiskError::VolumeGroupCreate { why })
    }

    pub(crate) fn create_partitions(&mut self) -> Result<(), DiskError> {
        let nparts = self.partitions.len();
        let volume_group = &self.volume_group;
        for (id, partition) in self.partitions.iter_mut().enumerate() {
            let label = partition.name.as_ref().unwrap();

            // Create the new logical volume on the volume group.
            lvcreate(
                &self.volume_group,
                &label,
                if id == nparts {
                    None
                } else {
                    Some((partition.end_sector - partition.start_sector) * self.sector_size)
                },
            ).map_err(|why| DiskError::LogicalVolumeCreate { why })?;

            // Set the device path of the newly-created partition.
            partition.device_path =
                PathBuf::from(format!("/dev/mapper/{}-{}", volume_group, label));

            // Then format the newly-created logical volume
            if let Some(fs) = partition.filesystem.as_ref() {
                mkfs(&partition.device_path, fs.clone())
                    .map_err(|why| DiskError::PartitionFormat { why })?;
            }
        }

        Ok(())
    }

    pub(crate) fn encrypt(&self) -> Result<(), DiskError> {
        if let Some(encryption) = self.encryption.as_ref() {
            if let Some(password) = encryption.password.as_ref() {
                unimplemented!();
            }

            if let Some(keyfile) = encryption.keyfile.as_ref() {
                unimplemented!();
            }
        }

        Ok(())
    }

    pub(crate) fn open(&self) -> Result<(), DiskError> {
        if let Some(enc) = self.encryption.as_ref() {
            match (enc.password.as_ref(), enc.keyfile.as_ref()) {
                (Some(password), None) => unimplemented!(),
                (Some(password), Some(keyfile)) => unimplemented!(),
                (None, Some(keyfile)) => unimplemented!(),
                (None, None) => unimplemented!(),
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LvmEncryption {
    pub(crate) password: Option<String>,
    pub(crate) keyfile:  Option<String>,
}
