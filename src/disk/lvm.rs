use super::{DiskError, DiskExt, PartitionInfo, PartitionTable, PartitionType};

#[derive(Debug, Clone, PartialEq)]
pub struct LvmDevice {
    pub(crate) volume_group: String,
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

    fn validate_partition_table(&self, _part_type: PartitionType) -> Result<(), DiskError> {
        Ok(())
    }

    fn push_partition(&mut self, partition: PartitionInfo) { self.partitions.push(partition); }
}

impl LvmDevice {
    pub(crate) fn new(volume_group: String, sectors: u64, sector_size: u64) -> LvmDevice {
        LvmDevice {
            volume_group,
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct LvmEncryption {
    pub(crate) password: Option<String>,
    pub(crate) keyfile:  Option<String>,
}