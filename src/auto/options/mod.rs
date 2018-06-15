//! Generate a list of installation options based on what already exists on the disk.

mod apply;
mod erase_option;
mod recovery_option;
mod refresh_option;

pub use self::apply::*;
pub use self::erase_option::*;
pub use self::recovery_option::*;
pub use self::refresh_option::*;

use std::path::PathBuf;

use super::super::*;
use misc::get_uuid;

#[derive(Debug)]
pub struct InstallOptions {
    pub refresh_options: Vec<RefreshOption>,
    pub erase_options:   Vec<EraseOption>,
    // pub multiboot_options: Vec<MultiBootOption>,
    pub recovery_option: Option<RecoveryOption>,
}

impl InstallOptions {
    /// Detects existing installations, and suggests new ones.
    ///
    /// Note that encrypted partitions will need to be decrypted within the `disks` object
    /// in order for the installed operating systems on them to be detected and reinstalled to.
    pub fn new(disks: &Disks, required_space: u64) -> InstallOptions {
        let mut erase_options = Vec::new();
        let mut refresh_options = Vec::new();

        let recovery_option = detect_recovery();

        {
            let erase_options = &mut erase_options;
            let refresh_options = &mut refresh_options;

            let mut check_partition = |part: &PartitionInfo| {
                if part.is_linux_compatible() {
                    match part.probe_os() {
                        Some(os) => match os {
                            OS::Linux {
                                info,
                                home,
                                efi,
                                recovery,
                            } => refresh_options.push(RefreshOption {
                                os_name:        info.name,
                                os_pretty_name: info.pretty_name,
                                os_version:     info.version,
                                root_part:      get_uuid(part.get_device_path())
                                    .expect("root device did not have uuid"),
                                home_part:      home,
                                efi_part:       efi,
                                recovery_part:  recovery,
                            }),
                            _ => (),
                        },
                        None => (),
                    }
                }
            };

            for device in disks.get_physical_devices() {
                if !Path::new("/cdrom/recovery.conf").exists() {
                    if device.contains_mount("/", &disks) || device.contains_mount("/cdrom", &disks) {
                        continue
                    }
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

                for part in device.get_partitions() {
                    check_partition(part);
                }
            }

            for device in disks.get_logical_devices() {
                for part in device.get_partitions() {
                    check_partition(part);
                }
            }
        }

        InstallOptions {
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
