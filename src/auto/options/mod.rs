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

use disk_types::{PartitionExt, SectorExt};
use disks::*;
use std::path::PathBuf;
use super::super::*;
use partition_identity::PartitionID;

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
    pub fn new(disks: &Disks, required_space: u64, shrink_overhead: u64) -> InstallOptions {
        let mut erase_options = Vec::new();
        let mut refresh_options = Vec::new();
        let mut alongside_options = Vec::new();

        let recovery_option = detect_recovery();

        {
            let erase_options = &mut erase_options;
            let refresh_options = &mut refresh_options;

            let mut check_partition = |part: &PartitionInfo| -> Option<OS> {
                if part.is_linux_compatible() {
                    if let Some(os) = part.probe_os() {
                        if let OS::Linux { ref info, ref partitions, ref targets } = os {
                            let home = targets.iter().position(|t| t == Path::new("/home"));
                            let efi = targets.iter().position(|t| t == Path::new("/boot/efi"));
                            let recovery = targets.iter().position(|t| t == Path::new("/recovery"));
                            refresh_options.push(RefreshOption {
                                os_release:     info.clone(),
                                root_part:      PartitionID::get_uuid(part.get_device_path())
                                    .expect("root device did not have uuid").id,
                                home_part:      home.map(|pos| partitions[pos].clone()),
                                efi_part:       efi.map(|pos| partitions[pos].clone()),
                                recovery_part:  recovery.map(|pos| partitions[pos].clone()),
                                can_retain_old: if let Ok(used) = part.sectors_used() {
                                     part.get_sectors() - used > required_space
                                } else {
                                    false
                                }
                            });
                        }

                        return Some(os);
                    }
                }

                None
            };

            for device in disks.get_physical_devices() {
                if device.is_read_only() || device.contains_mount("/", &disks) {
                    continue
                }

                let mut last_end_sector = 1024;

                for part in device.get_partitions() {
                    if let Ok(used) = part.sectors_used() {
                        let sectors = part.get_sectors();
                        let free = sectors - used;
                        if required_space + shrink_overhead < free {
                            let os = check_partition(part);
                            alongside_options.push(AlongsideOption {
                                device: device.get_device_path().to_path_buf(),
                                alongside: os,
                                method: AlongsideMethod::Shrink {
                                    path: part.get_device_path().to_path_buf(),
                                    partition: part.number,
                                    sectors_free: free,
                                    sectors_total: sectors
                                }
                            });
                        }
                    }

                    if required_space < part.start_sector - last_end_sector {
                        alongside_options.push(AlongsideOption {
                            device: device.get_device_path().to_path_buf(),
                            alongside: None,
                            method: AlongsideMethod::Free(Region::new(last_end_sector + 1, part.start_sector - 1))
                        })
                    }

                    last_end_sector = part.end_sector;
                }

                let last_sector = device.get_sectors () - 2048;
                if required_space < last_sector - last_end_sector {
                    alongside_options.push(AlongsideOption {
                        device: device.get_device_path().to_path_buf(),
                        alongside: None,
                        method: AlongsideMethod::Free(Region::new(last_end_sector + 1, last_sector))
                    })
                }

                let skip = ! Path::new("/cdrom/recovery.conf").exists()
                    && (
                        device.contains_mount("/", &disks)
                        || device.contains_mount("/cdrom", &disks)
                    );

                if skip {
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
            }

            for device in disks.get_logical_devices() {
                for part in device.get_partitions() {
                    check_partition(part);
                }
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
    #[fail(display = "partition ID ({:?}) was not found", id)]
    PartitionIDNotFound { id: PartitionID },
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
    #[fail(display = "EFI partition is required, but not found on this option")]
    RefreshWithoutEFI,
    #[fail(display = "Failed to retrieve list of mounts from /proc/mounts: {}", why)]
    ProcMounts { why: io::Error }
}

impl From<DiskError> for InstallOptionError {
    fn from(why: DiskError) -> InstallOptionError { InstallOptionError::DiskError { why } }
}

impl From<PartitionError> for InstallOptionError {
    fn from(why: PartitionError) -> InstallOptionError { InstallOptionError::PartitionError { why } }
}
