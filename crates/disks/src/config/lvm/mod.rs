mod encryption;
pub use self::encryption::LuksEncryption;
use super::{
    super::{
        DiskError, DiskExt, PartitionError, PartitionInfo, PartitionTable, PartitionType, FORMAT,
        REMOVE, SOURCE,
    },
    get_size,
};
use disk_types::{BlockDeviceExt, PartitionTableExt, SectorExt};
pub use crate::external::deactivate_devices;
use crate::external::{blkid_partition, lvcreate, lvremove, lvs, mkfs, vgactivate, vgcreate};
use partition_identity::PartitionIdentifiers;
use proc_mounts::MOUNTS;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct VgData {
    /// Size of a PE, measured in sectors.
    pe_size:  u64,
    // Total amount of PEs in this VG.
    total_pe: u64,
    /// PEs which have been allocated / used.
    alloc_pe: u64,
    /// PEs which are free.
    free_pe:  u64,
}

/// An LVM device acts similar to a Disk, but consists of one more block devices
/// that comprise a volume group, and may optionally be encrypted.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LogicalDevice {
    pub model_name:   String,
    pub volume_group: String,
    pub device_path:  PathBuf,
    pub luks_parent:  Option<PathBuf>,
    pub mount_point:  Vec<PathBuf>,
    pub file_system:  Option<PartitionInfo>,
    pub sectors:      u64,
    pub sector_size:  u64,
    pub partitions:   Vec<PartitionInfo>,
    pub encryption:   Option<LuksEncryption>,
    pub has_lvm:      bool,
    pub is_source:    bool,
    pub remove:       bool,
    pub vg_data:      VgData,
}

impl BlockDeviceExt for LogicalDevice {
    fn get_device_path(&self) -> &Path { &self.device_path }

    fn get_mount_point(&self) -> &[PathBuf] { self.mount_point.as_ref() }
}

impl PartitionTableExt for LogicalDevice {
    fn get_partition_table(&self) -> Option<PartitionTable> { Some(PartitionTable::Gpt) }

    fn get_partition_type_count(&self) -> (usize, usize, bool) { (0, 0, false) }
}

impl SectorExt for LogicalDevice {
    fn get_sectors(&self) -> u64 {
        self.sectors
    }
}

impl DiskExt for LogicalDevice {
    const LOGICAL: bool = true;

    fn get_file_system(&self) -> Option<&PartitionInfo> { self.file_system.as_ref() }

    fn get_file_system_mut(&mut self) -> Option<&mut PartitionInfo> { self.file_system.as_mut() }

    fn set_file_system(&mut self, mut fs: PartitionInfo) {
        // Set the volume group + encryption to be the same as the parent.
        fs.lvm_vg = Some(self.volume_group.clone());
        fs.encryption = self.encryption.clone();

        self.file_system = Some(fs);
        self.partitions.clear();
    }

    fn get_model(&self) -> &str { &self.model_name }

    fn get_partitions_mut(&mut self) -> &mut [PartitionInfo] { &mut self.partitions }

    fn get_partitions(&self) -> &[PartitionInfo] { &self.partitions }

    fn push_partition(&mut self, partition: PartitionInfo) { self.partitions.push(partition); }
}

impl LogicalDevice {
    /// Creates a new volume group, with an optional encryption configuration.
    pub fn new(
        volume_group: String,
        encryption: Option<LuksEncryption>,
        sectors: u64,
        sector_size: u64,
        is_source: bool,
    ) -> LogicalDevice {
        let device_path = PathBuf::from(format!("/dev/mapper/{}", volume_group.replace("-", "--")));
        let mounts = MOUNTS.read().expect("unable to get mounts within LogicalDevice::new");

        eprintln!("Logical device of {} is {:?}", volume_group,device_path);

        LogicalDevice {
            model_name: ["LVM ", &volume_group].concat(),
            mount_point: mounts.0
                .iter()
                .filter(|mount| &mount.source == &device_path)
                .map(|m| m.dest.clone())
                .collect(),
            volume_group,
            device_path,
            sectors,
            sector_size,
            partitions: Vec::new(),
            encryption,
            is_source,
            remove: false,
            ..Default::default()
        }
    }

    pub fn add_sectors(&mut self, sectors: u64) { self.sectors += sectors; }

    #[rustfmt::skip]
    pub fn validate(&self) -> Result<(), DiskError> {
        if self.get_partitions().iter().any(|p| p.name.is_none()) {
            return Err(DiskError::VolumePartitionLacksLabel {
                device: self.get_device_path().to_path_buf()
            });
        }

        Ok(())
    }

    /// Creates the volume group using all of the supplied block devices as members of the
    /// group.
    pub fn create_volume_group<I, S>(&self, blocks: I) -> Result<(), DiskError>
    where
        I: Iterator<Item = S>,
        S: AsRef<OsStr>,
    {
        vgcreate(&self.volume_group, blocks).map_err(|why| DiskError::VolumeGroupCreate {
            device: self.get_device_path().to_path_buf(),
            vg: self.volume_group.clone(),
            why,
        })
    }

    pub fn get_pe_free(&self) -> u64 { self.vg_data.free_pe }

    pub fn get_pe_size_in_sectors(&self) -> u64 { self.vg_data.pe_size }

    pub fn shrink_vg(&mut self, _pes: u64) -> Result<(), DiskError> { Ok(()) }

    pub fn shrink_pv(&mut self, _sectors: u64) -> Result<(), DiskError> { Ok(()) }

    pub fn get_last_sector(&self) -> u64 {
        self.get_partitions()
            .iter()
            .rev()
            .find(|p| !p.flag_is_enabled(REMOVE))
            .map_or(0, |p| p.end_sector)
    }

    /// Obtains a partition by it's volume, with shared access.
    pub fn get_partition(&self, volume: &str) -> Option<&PartitionInfo> {
        self.partitions.iter().find(|p| {
            p.name.as_ref().expect("logical partitions should have names").as_str() == volume
        })
    }

    /// Obtains a partition by it's volume, with unique access.
    pub fn get_partition_mut(&mut self, volume: &str) -> Option<&mut PartitionInfo> {
        self.partitions.iter_mut().find(|p| {
            p.name.as_ref().expect("logical partitions should have names").as_str() == volume
        })
    }

    pub fn add_partitions(&mut self) {
        info!("adding partitions to LVM device");
        let mut start_sector = 0;
        let _ = vgactivate(&self.volume_group);
        if let Ok(logical_paths) = lvs(&self.volume_group) {
            for path in logical_paths {
                // Wait for the device to be initialized, with a 5 second timeout.
                let mut nth = 0;
                while !path.exists() {
                    info!("waiting 1 second because {:?} does not exist yet", path);
                    if nth == 5 {
                        break;
                    }
                    nth += 1;
                    thread::sleep(Duration::from_millis(1000));
                }

                let length = match get_size(&path) {
                    Ok(length) => length,
                    Err(why) => {
                        eprintln!("unable to get size of LVM device {:?}: {}", path, why);
                        0
                    }
                };

                let identifiers = PartitionIdentifiers::from_path(&path);

                let device_path = match path.canonicalize() {
                    Ok(resolved) => resolved,
                    Err(why) => {
                        eprintln!("LVM device path is not a symbolic link: {}", why);
                        continue
                    }
                };

                eprintln!("Found logical device {:?}: {:?}", path, device_path);

                let partition = PartitionInfo {
                    bitflags: SOURCE,
                    number: -1,
                    ordering: -1,
                    start_sector,
                    end_sector: start_sector + length,
                    part_type: PartitionType::Primary,
                    flags: vec![],
                    filesystem: blkid_partition(&path),
                    name: {
                        let dev = path
                            .file_name()
                            .expect("logical partitions should have names")
                            .to_str()
                            .unwrap();
                        let value = dev.find('-').map_or(0, |v| v + 1);
                        Some(dev.split_at(value).1.into())
                    },
                    device_path,
                    mount_point: Vec::new(),
                    target: None,
                    original_vg: None,
                    lvm_vg: None,
                    encryption: None,
                    key_id: None,
                    identifiers,
                    subvolumes: std::collections::HashMap::new(),
                };

                start_sector += length + 1;
                self.partitions.push(partition);
            }
        }
    }

    pub fn set_luks_parent(&mut self, device: PathBuf) { self.luks_parent = Some(device); }

    pub fn clear_partitions(&mut self) {
        for partition in &mut self.partitions {
            partition.remove();
        }
    }

    pub fn remove_partition(&mut self, volume: &str) -> Result<(), DiskError> {
        let partitions = &mut self.partitions;
        let vg = self.volume_group.as_str();

        match partitions.iter_mut().find(|p| {
            p.name.as_ref().expect("logical partitions should have names").as_str() == volume
        }) {
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
    pub fn modify_partitions(&self) -> Result<(), DiskError> {
        let nparts = if self.partitions.is_empty() {
            if self.file_system.is_some() {
                0
            } else {
                return Ok(());
            }
        } else {
            self.partitions.len() - 1
        };

        let partitions = self
            .file_system
            .as_ref()
            .into_iter()
            .map(|part| (0, part))
            .chain(self.partitions.iter().enumerate());

        for (id, partition) in partitions {
            let label =
                partition.name.as_ref().expect("logical partitions should have names").as_str();

            // Don't create a partition if it already exists.
            if !partition.flag_is_enabled(SOURCE) {
                lvcreate(
                    &self.volume_group,
                    label,
                    if id == nparts {
                        None
                    } else {
                        Some(partition.get_sectors() * self.sector_size)
                    },
                )
                .map_err(|why| DiskError::LogicalVolumeCreate { why })?;
            }

            if partition.flag_is_enabled(REMOVE) {
                lvremove(&self.volume_group, label)
                    .map_err(|why| DiskError::PartitionRemove { partition: -1, why })?;
            } else if partition.flag_is_enabled(FORMAT) {
                if let Some(fs) = partition.filesystem {
                    mkfs(&partition.device_path, fs).map_err(|why| {
                        DiskError::new_partition_error(
                            partition.device_path.clone(),
                            PartitionError::PartitionFormat { why },
                        )
                    })?;
                }
            }
        }

        Ok(())
    }
}
