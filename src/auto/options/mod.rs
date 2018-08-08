//! Generate a list of installation options based on what already exists on the disk.

mod alongside_option;
mod apply;
mod erase_option;
mod recovery_option;
mod refresh_option;

pub use self::alongside_option::*;
pub use self::apply::*;
pub use self::erase_option::*;
pub use self::recovery_option::*;
pub use self::refresh_option::*;

use std::path::PathBuf;

use super::super::*;
use misc::get_uuid;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

#[derive(Debug)]
pub struct InstallOptions {
    pub alongside_options: Vec<AlongsideOption>,
    pub erase_options:     Vec<EraseOption>,
    pub recovery_option:   Option<RecoveryOption>,
    pub refresh_options:   Vec<RefreshOption>,
}

impl InstallOptions {
    /// Detects existing installations, and suggests new ones.
    ///
    /// Note that encrypted partitions will need to be decrypted within the `disks` object
    /// in order for the installed operating systems on them to be detected and reinstalled to.
    pub fn new(disks: &Disks, required_space: u64) -> InstallOptions {
        let mut erase_options = Vec::new();
        let mut refresh_options = Vec::new();

        let mut other_os: HashMap<&Path, AlongsideData> = HashMap::new();

        let recovery_option = detect_recovery();

        {
            let erase_options = &mut erase_options;
            let refresh_options = &mut refresh_options;

            let mut check_partition = |part: &PartitionInfo| -> Option<OS> {
                if part.is_linux_compatible() {
                    match part.probe_os() {
                        Some(os) => {
                            if let OS::Linux { ref info, ref home, ref efi, ref recovery } = os {
                                refresh_options.push(RefreshOption {
                                    os_name:        info.name.clone(),
                                    os_pretty_name: info.pretty_name.clone(),
                                    os_version:     info.version.clone(),
                                    root_part:      get_uuid(part.get_device_path())
                                        .expect("root device did not have uuid"),
                                    home_part:      home.clone(),
                                    efi_part:       efi.clone(),
                                    recovery_part:  recovery.clone(),
                                });
                            }

                            return Some(os);
                        },
                        None => (),
                    }
                }

                None
            };

            for device in disks.get_physical_devices() {
                let has_recovery = !Path::new("/cdrom/recovery.conf").exists()
                    && (device.contains_mount("/", &disks) || device.contains_mount("/cdrom", &disks));

                if has_recovery {
                    continue
                }

                let sectors = device.get_sectors();
                erase_options.push(EraseOption {
                    device: device.get_device_path().to_path_buf(),
                    model: {
                        let model = device.get_model();
                        if model.is_empty() {
                            device.get_serial().replace("_", " ")
                        } else {
                            model.into()
                        }
                    },
                    sectors,
                    flags: {
                        let mut flags = if device.is_removable() {
                            IS_REMOVABLE
                        } else {
                            0
                        };
                        flags |= if device.is_rotational() {
                            IS_ROTATIONAL
                        } else {
                            0
                        };

                        flags |= if sectors >= required_space || required_space == 0 {
                            MEETS_REQUIREMENTS
                        } else {
                            0
                        };
                        flags
                    },
                });

                let mut last_end_sector = 0;
                let mut best_free_region = Region::new(0, 0);

                for part in device.get_partitions() {
                    if let Some(os) = check_partition(part) {
                        match other_os.entry(device.get_device_path()) {
                            Entry::Occupied(mut entry) => entry.get_mut().systems.push(os),
                            Entry::Vacant(entry) => {
                                entry.insert(AlongsideData {
                                    systems: vec![os],
                                    largest_partition: -1,
                                    sectors_free: 0,
                                    best_free_region: Region::new(0, 0)
                                });
                            }
                        }
                    }

                    best_free_region.compare(last_end_sector, part.start_sector);
                    last_end_sector = part.end_sector;

                    if let Some(Ok(used)) = part.sectors_used(512) {
                        let free = part.sectors() - used;
                        let num = part.number;
                        match other_os.entry(device.get_device_path()) {
                            Entry::Occupied(mut entry) => {
                                let entry = entry.get_mut();
                                if entry.sectors_free < free {
                                    entry.largest_partition = num;
                                    entry.sectors_free = free;
                                }
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(AlongsideData {
                                    systems: Vec::new(),
                                    largest_partition: num,
                                    sectors_free: free,
                                    best_free_region: Region::new(0, 0),
                                });
                            }
                        }
                    }
                }

                match other_os.entry(device.get_device_path()) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().best_free_region = best_free_region;
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(AlongsideData {
                            systems: Vec::new(),
                            largest_partition: -1,
                            sectors_free: 0,
                            best_free_region,
                        });
                    }
                }
            }

            for device in disks.get_logical_devices() {
                for part in device.get_partitions() {
                    check_partition(part);
                }
            }
        }

        let mut alongside_options = Vec::new();
        for (device, data) in other_os.into_iter() {
            if data.systems.len() == 1 {
                if required_space < data.sectors_free {
                    alongside_options.push(AlongsideOption {
                        device: device.to_path_buf(),
                        alongside: data.systems[0].clone(),
                        method: AlongsideMethod::Shrink {
                            partition: data.largest_partition,
                            sectors_free: data.sectors_free,
                        }
                    });
                }

                if required_space < data.best_free_region.size() {
                    alongside_options.push(AlongsideOption {
                        device: device.to_path_buf(),
                        alongside: data.systems[0].clone(),
                        method: AlongsideMethod::Free(data.best_free_region)
                    });
                }
            } else {
                // TODO: What do we do when there are multiple installed OS's on the same disk?
            }
        }

        InstallOptions {
            alongside_options,
            erase_options,
            refresh_options,
            recovery_option,
        }
    }
}

#[derive(Debug, Fail)]
pub enum InstallOptionError {
    #[fail(display = "partition ({}) was not found in disks object", uuid)]
    PartitionNotFound { uuid: String },
    #[fail(display = "partition {} was not found in {:?}", number, device)]
    PartitionNotFoundByID { number: i32, device: PathBuf },
    #[fail(display = "partition error: {}", why)]
    PartitionError { why: PartitionError },
    #[fail(display = "device ({:?}) was not found in disks object", path)]
    DeviceNotFound { path: PathBuf },
    #[fail(display = "logical device was not found by the volume group ({})", vg)]
    LogicalDeviceNotFound { vg: String },
    #[fail(display = "error applying changes to disks: {}", why)]
    DiskError { why: DiskError },
    #[fail(display = "error generating volume group ID: {}", why)]
    GenerateID { why: io::Error },
    #[fail(display = "recovery does not have LVM partition")]
    RecoveryNoLvm,
}

impl From<DiskError> for InstallOptionError {
    fn from(why: DiskError) -> InstallOptionError { InstallOptionError::DiskError { why } }
}

impl From<PartitionError> for InstallOptionError {
    fn from(why: PartitionError) -> InstallOptionError { InstallOptionError::PartitionError { why } }
}
