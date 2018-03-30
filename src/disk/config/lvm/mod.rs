mod detect;
mod encryption;

pub(crate) use self::detect::physical_volumes_to_deactivate;
pub use self::encryption::LvmEncryption;
use super::super::external::{dmlist, lvcreate, lvremove, mkfs, vgcreate};
use super::super::mounts::Mounts;
use super::super::{DiskError, DiskExt, PartitionInfo, PartitionTable, PartitionType, FORMAT, REMOVE, SOURCE};
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use rand::{self, Rng};

pub fn generate_unique_id(prefix: &str) -> io::Result<String> {
    let dmlist = dmlist()?;
    loop {
        let id: String = rand::thread_rng()
            .gen_ascii_chars()
            .take(5)
            .collect();
        let id = [prefix, "-", &id].concat();
        if dmlist.contains(&id) {
            continue
        }
        return Ok(id);
    }
}

/// An LVM device acts similar to a Disk, but consists of one more block devices
/// that comprise a volume group, and may optionally be encrypted.
#[derive(Debug, Clone, PartialEq)]
pub struct LvmDevice {
    pub(crate) model_name:   String,
    pub(crate) volume_group: String,
    pub(crate) device_path:  PathBuf,
    pub(crate) mount_point:  Option<PathBuf>,
    pub(crate) sectors:      u64,
    pub(crate) sector_size:  u64,
    pub(crate) partitions:   Vec<PartitionInfo>,
    pub(crate) encryption:   Option<LvmEncryption>,
    pub(crate) is_source:    bool,
    pub(crate) remove:       bool,
}

impl DiskExt for LvmDevice {
    const LOGICAL: bool = true;

    fn get_device_path(&self) -> &Path { &self.device_path }

    fn get_model(&self) -> &str { &self.model_name }

    fn get_mount_point(&self) -> Option<&Path> { self.mount_point.as_ref().map(|x| x.as_path()) }

    fn get_partitions_mut(&mut self) -> &mut [PartitionInfo] { &mut self.partitions }

    fn get_partitions(&self) -> &[PartitionInfo] { &self.partitions }

    fn get_sector_size(&self) -> u64 { self.sector_size }

    fn get_sectors(&self) -> u64 { self.sectors }

    fn get_table_type(&self) -> Option<PartitionTable> { None }

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
        is_source: bool,
    ) -> LvmDevice {
        let device_path = PathBuf::from(format!("/dev/mapper/{}", volume_group));

        // TODO: Optimize this so it's not called for each disk.
        let mounts = Mounts::new().unwrap();

        LvmDevice {
            model_name: ["LVM ", &volume_group].concat(),
            mount_point: mounts.get_mount_point(&device_path),
            volume_group,
            device_path,
            sectors,
            sector_size,
            partitions: Vec::new(),
            encryption,
            is_source,
            remove: false,
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

    pub fn get_last_sector(&self) -> u64 {
        self.get_partitions()
            .iter()
            .rev()
            .find(|p| !p.flag_is_enabled(REMOVE))
            .map_or(0, |p| p.end_sector)
    }

    /// Obtains a partition by it's volume, with shared access.
    pub fn get_partition(&self, volume: &str) -> Option<&PartitionInfo> {
        self.partitions
            .iter()
            .find(|p| p.name.as_ref().unwrap().as_str() == volume)
    }

    /// Obtains a partition by it's volume, with unique access.
    pub fn get_partition_mut(&mut self, volume: &str) -> Option<&mut PartitionInfo> {
        self.partitions
            .iter_mut()
            .find(|p| p.name.as_ref().unwrap().as_str() == volume)
    }

    pub fn clear_partitions(&mut self) {
        for partition in &mut self.partitions {
            partition.remove();
        }
    }

    pub fn remove_partition(&mut self, volume: &str) -> Result<(), DiskError> {
        let partitions = &mut self.partitions;
        let vg = self.volume_group.as_str();

        match partitions
            .iter_mut()
            .find(|p| p.name.as_ref().unwrap().as_str() == volume)
        {
            Some(partition) => {
                partition.remove();
                Ok(())
            }
            None => Err(DiskError::LogicalPartitionNotFound {
                group:  vg.into(),
                volume: volume.into(),
            }),
        }
    }

    /// Create & modify all logical volumes on the volume group, and format them.
    pub(crate) fn modify_partitions(&self) -> Result<(), DiskError> {
        if self.partitions.is_empty() {
            return Ok(());
        }
        let nparts = self.partitions.len() - 1;
        for (id, partition) in self.partitions.iter().enumerate() {
            let label = partition.name.as_ref().unwrap().as_str();

            // Don't create a partition if it already exists.
            if !partition.flag_is_enabled(SOURCE) {
                lvcreate(
                    &self.volume_group,
                    label,
                    if id == nparts {
                        None
                    } else {
                        Some(partition.sectors() * self.sector_size)
                    },
                ).map_err(|why| DiskError::LogicalVolumeCreate { why })?;
            }

            if partition.flag_is_enabled(REMOVE) {
                lvremove(&self.volume_group, label)
                    .map_err(|why| DiskError::PartitionRemove { partition: -1, why })?;
            } else if partition.flag_is_enabled(FORMAT) {
                if let Some(fs) = partition.filesystem.as_ref() {
                    mkfs(&partition.device_path, fs.clone())
                        .map_err(|why| DiskError::PartitionFormat { why })?;
                }
            }
        }

        Ok(())
    }
}
