mod deactivate;
mod detect;
mod encryption;

pub use self::deactivate::deactivate_devices;
pub use self::detect::physical_volumes_to_deactivate;
pub use self::encryption::LvmEncryption;
use external::{
    blkid_partition, dmlist, lvcreate, lvremove, lvs, mkfs, vgactivate, vgcreate,
};
use misc::hasher;
use proc_mounts::MOUNTS;
use super::super::{
    DiskError, DiskExt, PartitionError, PartitionInfo, PartitionTable,
    PartitionType, FORMAT, REMOVE, SOURCE,
};
use super::get_size;
use rand::{self, Rng};
use rand::distributions::Alphanumeric;
use std::ffi::OsStr;
use std::{io, thread};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Generate a unique device map ID, to ensure no collisions between dm blocks.
pub fn generate_unique_id(prefix: &str, exclude_hashes: &[u64]) -> io::Result<String> {
    let dmlist = dmlist()?;
    let check_uniqueness = |id: &str, exclude: &[u64]| -> bool {
        ! dmlist.iter().any(|x| x.as_str() == id) && ! exclude.contains(&hasher(&id))
    };

    if check_uniqueness(prefix, exclude_hashes) {
        return Ok(prefix.into());
    }

    loop {
        let id: String = rand::thread_rng().sample_iter(&Alphanumeric).take(5).collect();
        let id = [prefix, "_", &id].concat();
        if ! check_uniqueness(&id, exclude_hashes) {
            continue;
        }
        return Ok(id);
    }
}

// TODO: Change name to LogicalDevice?

/// An LVM device acts similar to a Disk, but consists of one more block devices
/// that comprise a volume group, and may optionally be encrypted.
#[derive(Debug, Clone, PartialEq)]
pub struct LvmDevice {
    pub model_name:   String,
    pub volume_group: String,
    pub device_path:  PathBuf,
    pub luks_parent:  Option<PathBuf>,
    pub mount_point:  Option<PathBuf>,
    pub file_system:  Option<PartitionInfo>,
    pub sectors:      u64,
    pub sector_size:  u64,
    pub partitions:   Vec<PartitionInfo>,
    pub encryption:   Option<LvmEncryption>,
    pub is_source:    bool,
    pub remove:       bool,
}

impl DiskExt for LvmDevice {
    const LOGICAL: bool = true;

    fn get_device_path(&self) -> &Path { &self.device_path }

    fn get_file_system(&self) -> Option<&PartitionInfo> { self.file_system.as_ref() }

    fn get_file_system_mut(&mut self) -> Option<&mut PartitionInfo> { self.file_system.as_mut() }

    fn set_file_system(&mut self, mut fs: PartitionInfo) {
        // Set the volume group + encryption to be the same as the parent.
        fs.volume_group = Some((self.volume_group.clone(), self.encryption.clone()));

        self.file_system = Some(fs);
        self.partitions.clear();
    }

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
    pub fn new(
        volume_group: String,
        encryption: Option<LvmEncryption>,
        sectors: u64,
        sector_size: u64,
        is_source: bool,
    ) -> LvmDevice {
        let device_path = PathBuf::from(format!("/dev/mapper/{}", volume_group.replace("-", "--")));
        let mounts = MOUNTS.read().expect("unable to get mounts within LvmDevice::new");

        LvmDevice {
            model_name: ["LVM ", &volume_group].concat(),
            mount_point: mounts.get_mount_point(&device_path),
            volume_group,
            device_path,
            luks_parent: None,
            file_system: None,
            sectors,
            sector_size,
            partitions: Vec::new(),
            encryption,
            is_source,
            remove: false,
        }
    }

    pub fn add_sectors(&mut self, sectors: u64) { self.sectors += sectors; }

    #[cfg_attr(rustfmt, rustfmt_skip)]
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
            why
        })
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
            .find(|p| p.name.as_ref().expect("logical partitions should have names").as_str() == volume)
    }

    /// Obtains a partition by it's volume, with unique access.
    pub fn get_partition_mut(&mut self, volume: &str) -> Option<&mut PartitionInfo> {
        self.partitions
            .iter_mut()
            .find(|p| p.name.as_ref().expect("logical partitions should have names").as_str() == volume)
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
                    info!(
                        "waiting 1 second because {:?} does not exist yet",
                        path
                    );
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
                        let dev = path.file_name().expect("logical partitions should have names").to_str().unwrap();
                        let value = dev.find('-').map_or(0, |v| v + 1);
                        Some(dev.split_at(value).1.into())
                    },
                    device_path: path,
                    mount_point: None,
                    target: None,
                    original_vg: None,
                    volume_group: None,
                    key_id: None,
                };

                start_sector += length + 1;
                self.partitions.push(partition);
            }
        }
    }

    pub fn set_luks_parent(&mut self, device: PathBuf) {
        self.luks_parent = Some(device);
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
            .find(|p| p.name.as_ref().expect("logical partitions should have names").as_str() == volume)
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

        let partitions = self.file_system.as_ref().into_iter()
            .map(|part| (0, part))
            .chain(self.partitions.iter().enumerate());

        for (id, partition) in partitions {
            let label = partition.name.as_ref().expect("logical partitions should have names").as_str();

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
                if let Some(fs) = partition.filesystem {
                    mkfs(&partition.device_path, fs)
                        .map_err(|why| DiskError::new_partition_error(
                            partition.device_path.clone(),
                            PartitionError::PartitionFormat { why }
                        ))?;
                }
            }
        }

        Ok(())
    }
}
