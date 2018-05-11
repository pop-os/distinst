//! Generate a list of installation options based on what already exists on the disk.

use std::path::PathBuf;

use super::super::{Bootloader, Disks, DiskExt, DiskError, OS};
use misc::get_uuid;

pub struct InstallOptions {
    pub options: Vec<InstallOption>,
    pub efi_partitions: Option<Vec<String>>,
    pub swap_partitions: Vec<String>
}

pub enum InstallOptionError {
    PartitionNotFound { uuid: String },
}

pub enum InstallOption {
    RefreshOption {
        os_name: String,
        os_version: String,
        root_part: String,
        root_sectors: u64,
        home_part: Option<String>,
        efi_part: Option<String>
    },
    EraseAndInstall {
        device: String,
        sectors: u64
    }
}

impl InstallOption {
    fn apply(self, disks: &mut Disks) -> Result<(), InstallOptionError> {
        match self {
            InstallOption::RefreshOption { root_part, home_part, efi_part, .. } => {
                {
                    let root = disks.get_partition_by_uuid_mut(&root_part)
                        .ok_or(InstallOptionError::PartitionNotFound { uuid: root_part })?;
                    root.set_mount("/".into());
                }

                if let Some(home) = home_part {
                    let home = disks.get_partition_by_uuid_mut(&home)
                        .ok_or(InstallOptionError::PartitionNotFound { uuid: home })?;
                    home.set_mount("/home".into());
                }

                if let Some(efi) = efi_part {
                    let efi = disks.get_partition_by_uuid_mut(&efi)
                        .ok_or(InstallOptionError::PartitionNotFound { uuid: efi })?;
                    efi.set_mount("/boot/efi".into());
                }
            },
            InstallOption::EraseAndInstall { device, .. } => {
                unimplemented!()
            }
        }

        Ok(())
    }
}

impl InstallOptions {
    pub fn new(disks: &Disks, required_space: u64) -> InstallOptions {
        let mut efi_partitions = if Bootloader::detect() == Bootloader::Efi {
            Some(vec![])
        } else {
            None
        };

        let mut swap_partitions = Vec::new();

        let mut options = Vec::new();

        for device in disks.get_physical_devices() {
            if device.get_sectors() >= required_space {
                options.push(InstallOption::EraseAndInstall {
                    device: get_uuid(device.get_device_path())
                        .expect("uuid not found for device"),
                    sectors: device.get_sectors()
                })
            }

            for part in device.get_partitions() {
                if part.is_esp_partition() {
                    if let Some(ref mut vec) = efi_partitions.as_mut() {
                        if let Some(uuid) = get_uuid(part.get_device_path()) {
                            vec.push(uuid);
                        }
                    }
                } else if part.is_swap() {
                    if let Some(uuid) = get_uuid(part.get_device_path()) {
                        swap_partitions.push(uuid);
                    }
                } else if part.is_linux_compatible() {
                    match part.probe_os() {
                        Some(os) => match os {
                            OS::Linux { info, home, efi } => {
                                options.push(InstallOption::RefreshOption {
                                    os_name: info.pretty_name,
                                    os_version: info.version,
                                    root_part: get_uuid(part.get_device_path())
                                        .expect("root device did not have uuid"),
                                    root_sectors: part.sectors(),
                                    home_part: home,
                                    efi_part: efi,
                                })
                            }
                            _ => ()
                        }
                        None => ()
                    }
                }
            }
        }

        InstallOptions { options, efi_partitions, swap_partitions }
    }
}
