use std::fmt;
use std::mem;

use super::super::super::*;
use super::{RefreshOption, EraseOption, InstallOptionError};

pub enum InstallOption<'a> {
    RefreshOption(&'a RefreshOption),
    EraseOption {
        option: &'a EraseOption,
        password: Option<String>,
    }
}

impl<'a> fmt::Debug for InstallOption<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            InstallOption::RefreshOption(ref option) => {
                write!(f, "InstallOption::RefreshOption({:?})", option)
            }
            InstallOption::EraseOption { ref option, .. } => {
                write!(f, "InstallOption::EraseOption {{ option: {:?}, password: hidden }}", option)
            }
        }
    }
}

impl<'a> InstallOption<'a> {
    /// Applies a given installation option to the `disks` object.
    ///
    /// If the option is to erase and install, the `disks` object will be replaced with a new one.
    pub fn apply(self, disks: &mut Disks) -> Result<(), InstallOptionError> {
        let bootloader = Bootloader::detect();

        match self {
            // Reuse existing partitions, without making any modifications.
            InstallOption::RefreshOption(option) => {
                {
                    let root = disks.get_partition_by_uuid_mut(&option.root_part)
                        .ok_or(InstallOptionError::PartitionNotFound { uuid: option.root_part.clone() })?;
                    root.set_mount("/".into());
                }

                if let Some(ref home) = option.home_part {
                    let home = disks.get_partition_by_uuid_mut(home)
                        .ok_or(InstallOptionError::PartitionNotFound { uuid: home.clone() })?;
                    home.set_mount("/home".into());
                }

                if let Some(ref efi) = option.efi_part {
                    let efi = disks.get_partition_by_uuid_mut(efi)
                        .ok_or(InstallOptionError::PartitionNotFound { uuid: efi.clone() })?;
                    efi.set_mount("/boot/efi".into());
                }

                if let Some(ref recovery) = option.recovery_part {
                    let recovery = disks.get_partition_by_uuid_mut(recovery)
                        .ok_or(InstallOptionError::PartitionNotFound { uuid: recovery.clone() })?;
                    recovery.set_mount("/recovery".into());
                }
            },
            // Reset the `disks` object and designate a disk to be wiped and installed.
            InstallOption::EraseOption { option, password } => {
                let mut tmp = Disks::new();
                mem::swap(&mut tmp, disks);

                let mut root_vg: Option<String> = None;

                let start_sector = Sector::Start;
                let boot_sector = Sector::Megabyte(512);
                let recovery_sector = Sector::Megabyte(512 + 4096);
                let swap_sector = Sector::MegabyteFromEnd(4096);
                let end_sector = Sector::End;

                let lvm = match password {
                    Some(pass) => {
                        let (root, encrypted_vg) = generate_unique_id("data")
                            .and_then(|r| generate_unique_id("cryptdata").map(|e| (r, e)))
                            .map_err(|why| InstallOptionError::GenerateID { why })?;

                        root_vg = Some(root.clone());

                        Some((LvmEncryption::new(encrypted_vg, Some(pass), None), root))
                    }
                    None => None
                };

                {
                    let mut device = Disk::from_name(&option.device).ok()
                        .ok_or(InstallOptionError::DeviceNotFound { path: option.device.clone() })?;

                    let result = match bootloader {
                        Bootloader::Efi => {
                            device.mklabel(PartitionTable::Gpt)
                                // Configure ESP partition
                                .and_then(|_| {
                                    let start = device.get_sector(start_sector);
                                    let end = device.get_sector(boot_sector);
                                    device.add_partition(
                                        PartitionBuilder::new(start, end, FileSystemType::Fat32)
                                            .partition_type(PartitionType::Primary)
                                            .flag(PartitionFlag::PED_PARTITION_ESP)
                                            .mount("/boot/efi".into())
                                    )
                                })
                                // Configure recovery partition
                                .and_then(|_| {
                                    let start = device.get_sector(boot_sector);
                                    let end = device.get_sector(recovery_sector);
                                    device.add_partition(
                                        PartitionBuilder::new(start, end, FileSystemType::Fat32)
                                            .name("recovery".into())
                                            .mount("/recovery".into())
                                    )
                                })
                                .map(|_| (
                                    device.get_sector(recovery_sector),
                                    device.get_sector(swap_sector)
                                ))
                        }
                        Bootloader::Bios => {
                            device.mklabel(PartitionTable::Msdos)
                                // This is used to ensure LVM installs will work with BIOS
                                .and_then(|_| if lvm.is_some() {
                                    let start = device.get_sector(start_sector);
                                    let end = device.get_sector(boot_sector);
                                    device.add_partition(
                                        PartitionBuilder::new(start, end, FileSystemType::Ext4)
                                            .partition_type(PartitionType::Primary)
                                            .flag(PartitionFlag::PED_PARTITION_BOOT)
                                            .mount("/boot".into())
                                    ).map(|_| (boot_sector, swap_sector))
                                } else {
                                    Ok((start_sector, swap_sector))
                                })
                                .map(|(start, end)| (
                                    device.get_sector(start),
                                    device.get_sector(end)
                                ))
                        }
                    };

                    // Configure optionally-encrypted root volume
                    result.and_then(|(start, end)| {
                        device.add_partition(if let Some((enc, root_vg)) = lvm {
                            PartitionBuilder::new(start, end, FileSystemType::Lvm)
                                .partition_type(PartitionType::Primary)
                                .logical_volume(root_vg, Some(enc))
                        } else {
                            PartitionBuilder::new(start, end, FileSystemType::Ext4)
                                .mount("/".into())
                        })
                    })
                    // Configure swap partition
                    .and_then(|_| {
                        let start = device.get_sector(swap_sector);
                        let end = device.get_sector(end_sector);
                        device.add_partition(
                            PartitionBuilder::new(start, end, FileSystemType::Swap)
                        )
                    })?;

                    disks.add(device);
                }

                disks.initialize_volume_groups()?;

                if let Some(root_vg) = root_vg {
                    let lvm_device = disks.get_logical_device_mut(&root_vg)
                        .ok_or(InstallOptionError::LogicalDeviceNotFound { vg: root_vg })?;

                    let start = lvm_device.get_sector(start_sector);
                    let end = lvm_device.get_sector(end_sector);

                    lvm_device.add_partition(
                        PartitionBuilder::new(start, end, FileSystemType::Ext4)
                            .name("root".into())
                            .mount("/".into())
                    )?;
                }
            }
        }

        Ok(())
    }
}
