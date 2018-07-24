use std::fmt;
use std::mem;

use super::super::super::*;
use super::{EraseOption, InstallOptionError, RecoveryOption, RefreshOption};
use misc;

pub enum InstallOption<'a> {
    RefreshOption(&'a RefreshOption),
    EraseOption {
        option:   &'a EraseOption,
        password: Option<String>,
    },
    RecoveryOption {
        option:   &'a RecoveryOption,
        password: Option<String>,
    },
}

impl<'a> fmt::Debug for InstallOption<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            InstallOption::RefreshOption(ref option) => {
                write!(f, "InstallOption::RefreshOption({:?})", option)
            }
            InstallOption::RecoveryOption { .. } => write!(f, "InstallOption::RecoveryOption"),
            InstallOption::EraseOption { ref option, .. } => write!(
                f,
                "InstallOption::EraseOption {{ option: {:?}, password: hidden }}",
                option
            ),
        }
    }
}

fn set_mount_by_uuid(disks: &mut Disks, uuid: &str, mount: &str) -> Result<(), InstallOptionError> {
    disks
        .get_partition_by_uuid_mut(uuid)
        .ok_or(InstallOptionError::PartitionNotFound { uuid: uuid.into() })
        .map(|part| {
            part.set_mount(mount.into());
            ()
        })
}

fn generate_encryption(
    password: Option<String>,
) -> Result<Option<(LvmEncryption, String)>, InstallOptionError> {
    let value = match password {
        Some(pass) => {
            let (root, encrypted_vg) = generate_unique_id("data")
                .and_then(|r| generate_unique_id("cryptdata").map(|e| (r, e)))
                .map_err(|why| InstallOptionError::GenerateID { why })?;

            let root_vg = root.clone();
            let enc = LvmEncryption::new(encrypted_vg, Some(pass), None);
            Some((enc, root_vg))
        }
        None => None,
    };

    Ok(value)
}

impl<'a> InstallOption<'a> {
    /// Applies a given installation option to the `disks` object.
    ///
    /// If the option is to erase and install, the `disks` object will be replaced with a new one.
    pub fn apply(self, disks: &mut Disks) -> Result<(), InstallOptionError> {

        match self {
            // Reuse existing partitions, without making any modifications.
            InstallOption::RefreshOption(option) => {
                refresh_config(disks, option)?;
            }
            InstallOption::RecoveryOption { option, password } => {
                recovery_config(disks, option, password)?;
            }
            // Reset the `disks` object and designate a disk to be wiped and installed.
            InstallOption::EraseOption { option, password } => {
                erase_config(disks, option, password)?;
            }
        }

        Ok(())
    }
}

/// Apply a `refresh` config to `disks`.
fn refresh_config(disks: &mut Disks, option: &RefreshOption) -> Result<(), InstallOptionError> {
    {
        let root = disks.get_partition_by_uuid_mut(&option.root_part).ok_or(
            InstallOptionError::PartitionNotFound {
                uuid: option.root_part.clone(),
            },
        )?;
        root.set_mount("/".into());
    }

    if let Some(ref home) = option.home_part {
        set_mount_by_uuid(disks, home, "/home")?;
    }

    if let Some(ref efi) = option.efi_part {
        set_mount_by_uuid(disks, efi, "/boot/efi")?;
    }

    if let Some(ref recovery) = option.recovery_part {
        set_mount_by_uuid(disks, recovery, "/recovery")?;
    }

    Ok(())
}

/// Apply a `recovery mode` config to `disks`.
fn recovery_config(
    disks: &mut Disks,
    option: &RecoveryOption,
    password: Option<String>
) -> Result<(), InstallOptionError> {
    let mut tmp = Disks::default();
    mem::swap(&mut tmp, disks);

    let (lvm, root_vg) = match generate_encryption(password)? {
        Some((enc, root)) => (Some((enc, root.clone())), Some(root)),
        None => (None, None),
    };

    let mut recovery_device: Disk = {
        let mut recovery_path: PathBuf = misc::from_uuid(&option.recovery_uuid)
            .ok_or_else(|| InstallOptionError::PartitionNotFound {
                uuid: option.recovery_uuid.clone()
            })?;

        if let Some(physical) = misc::resolve_to_physical(recovery_path.file_name().unwrap().to_str().unwrap()) {
            recovery_path = physical;
        }

        if let Some(parent) = misc::resolve_parent(recovery_path.file_name().unwrap().to_str().unwrap()) {
            recovery_path = parent;
        }

        info!("recovery disk found at {:?}", recovery_path);
        Disk::from_name(&recovery_path)
            .ok()
            .ok_or(InstallOptionError::DeviceNotFound {
                path: recovery_path.to_path_buf(),
            })?
    };

    {
        let recovery_device = &mut recovery_device;
        let lvm_part: Option<PathBuf> = option.luks_uuid.as_ref()
            .and_then(|ref uuid| misc::from_uuid(uuid));

        if let Some(ref uuid) = option.efi_uuid {
            let path = &misc::from_uuid(uuid).expect("no uuid for efi part");
            recovery_device
                .get_partitions_mut()
                .iter_mut()
                .find(|d| d.get_device_path() == path)
                .ok_or(InstallOptionError::PartitionNotFound { uuid: uuid.clone() })
                .map(|part| part.set_mount("/boot/efi".into()))?;
        }

        {
            let uuid = &option.recovery_uuid;
            let path = &misc::from_uuid(uuid).expect("no uuid for recovery part");
            recovery_device
                .get_partitions_mut()
                .iter_mut()
                .find(|d| d.get_device_path() == path)
                .ok_or(InstallOptionError::PartitionNotFound { uuid: uuid.clone() })?;
        }

        let (start, end);

        let root_path = if let Some(mut part) = lvm_part {
            if let Some(physical) = misc::resolve_to_physical(part.file_name().unwrap().to_str().unwrap()) {
                part = physical;
            }

            part
        } else {
            misc::from_uuid(&option.root_uuid).ok_or_else(|| InstallOptionError::PartitionNotFound {
                uuid: option.root_uuid.clone()
            })?
        };

        let id = {
            let part = recovery_device
                .get_partitions()
                .iter()
                .find(|d| d.get_device_path() == root_path)
                .ok_or(InstallOptionError::PartitionNotFound {
                    uuid: root_path.to_string_lossy().to_string(),
                })?;

            start = part.start_sector;
            end = part.end_sector;
            part.number
        };

        recovery_device.remove_partition(id)?;

        if let Some((enc, root)) = lvm {
            recovery_device.add_partition(
                PartitionBuilder::new(start, end, FileSystemType::Luks)
                    .logical_volume(root, Some(enc)),
            )?;
        } else {
            recovery_device.add_partition(
                PartitionBuilder::new(start, end, FileSystemType::Ext4)
                    .mount("/".into()),
            )?;
        }
    }

    disks.add(recovery_device);
    disks.initialize_volume_groups()?;

    if let Some(root_vg) = root_vg {
        let lvm_device = disks
            .get_logical_device_mut(&root_vg)
            .ok_or(InstallOptionError::LogicalDeviceNotFound { vg: root_vg })?;

        let start = lvm_device.get_sector(Sector::Start);
        let end = lvm_device.get_sector(Sector::End);

        lvm_device.add_partition(
            PartitionBuilder::new(start, end, FileSystemType::Ext4)
                .name("root".into())
                .mount("/".into()),
        )?;
    }

    Ok(())
}

/// Apply an "erase and install" configuration to `disks`;
fn erase_config(
    disks: &mut Disks,
    option: &EraseOption,
    password: Option<String>
) -> Result<(), InstallOptionError> {
    let mut tmp = Disks::default();
    mem::swap(&mut tmp, disks);

    let bootloader = Bootloader::detect();

    let start_sector = Sector::Start;
    let boot_sector = Sector::Megabyte(512);
    let recovery_sector = Sector::Megabyte(512 + 4096);
    let swap_sector = Sector::MegabyteFromEnd(4096);
    let end_sector = Sector::End;

    let (lvm, root_vg) = match generate_encryption(password)? {
        Some((enc, root)) => (Some((enc, root.clone())), Some(root)),
        None => (None, None),
    };

    {
        let mut device = Disk::from_name(&option.device).ok().ok_or(
            InstallOptionError::DeviceNotFound {
                path: option.device.clone(),
            },
        )?;

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
        let lvm_device = disks
            .get_logical_device_mut(&root_vg)
            .ok_or(InstallOptionError::LogicalDeviceNotFound { vg: root_vg })?;

        let start = lvm_device.get_sector(start_sector);
        let end = lvm_device.get_sector(end_sector);

        lvm_device.add_partition(
            PartitionBuilder::new(start, end, FileSystemType::Ext4)
                .name("root".into())
                .mount("/".into()),
        )?;
    }

    Ok(())
}
