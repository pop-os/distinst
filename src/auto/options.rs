//! Generate a list of installation options based on what already exists on the disk.

use std::path::PathBuf;

use super::super::{Bootloader, Disks, DiskExt, DiskError, OS};

pub enum InstallOptionsError {
    DiskProbe { why: DiskError },
    LvmInitialize { why: DiskError }
}

pub struct InstallOptions {
    pub options: Vec<InstallOption>,
    pub efi_partitions: Option<Vec<PathBuf>>,
    pub swap_partitions: Vec<PathBuf>
}

pub enum InstallOption {
    RefreshOption {
        os_name: String,
        os_version: String,
        root_partition: PathBuf,
        root_sectors: u64,
        home_partition: Option<PathBuf>,
        efi_partition: Option<PathBuf>
    },
    EraseAndInstall {
        device: PathBuf,
        sectors: u64
    }
}

impl InstallOptions {
    pub fn detect(required_space: u64) -> Result<InstallOptions, InstallOptionsError> {
        let mut disks = Disks::probe_devices()
            .map_err(|why| InstallOptionsError::DiskProbe { why })?;

        disks.initialize_volume_groups()
            .map_err(|why| InstallOptionsError::LvmInitialize { why })?;

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
                    device: device.get_device_path().to_path_buf(),
                    sectors: device.get_sectors()
                })
            }

            for part in device.get_partitions() {
                if part.is_esp_partition() {
                    if let Some(ref mut vec) = efi_partitions.as_mut() {
                        vec.push(part.get_device_path().to_path_buf());
                    }
                } else if part.is_swap() {
                    swap_partitions.push(part.get_device_path().to_path_buf());
                } else if part.is_linux_compatible() {
                    match part.probe_os() {
                        Some(os) => match os {
                            OS::Linux { info, home, efi } => {
                                options.push(InstallOption::RefreshOption {
                                    os_name: info.pretty_name,
                                    os_version: info.version,
                                    root_partition: part.get_device_path().to_path_buf(),
                                    root_sectors: part.sectors(),
                                    home_partition: home,
                                    efi_partition: efi,
                                })
                            }
                            _ => ()
                        }
                        None => ()
                    }
                }
            }
        }

        Ok(InstallOptions { options, efi_partitions, swap_partitions })
    }
}
