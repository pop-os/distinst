use std::fmt;
use std::mem;

use super::super::super::*;
use super::{AlongsideOption, EraseOption, InstallOptionError, RecoveryOption, RefreshOption};
use misc;

pub enum InstallOption<'a> {
    Alongside {
        option: &'a AlongsideOption,
        password: Option<String>,
        sectors: u64,
    },
    Refresh(&'a RefreshOption),
    Erase {
        option:   &'a EraseOption,
        password: Option<String>,
    },
    Recovery {
        option:   &'a RecoveryOption,
        password: Option<String>,
    },
}

impl<'a> fmt::Debug for InstallOption<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            InstallOption::Alongside { ref option, .. } => {
                write!(f, "InstallOption::Alongside {{ option: {:?}, .. }}", option)
            }
            InstallOption::Refresh(ref option) => {
                write!(f, "InstallOption::RefreshOption({:?})", option)
            }
            InstallOption::Recovery { .. } => write!(f, "InstallOption::RecoveryOption"),
            InstallOption::Erase { ref option, .. } => write!(
                f,
                "InstallOption::EraseOption {{ option: {:?}, .. }}",
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
            // Install alongside another OS, taking `sectors` from the largest free partition.
            InstallOption::Alongside { option, password, sectors } => {
                alongside_config(disks, option, password, sectors)?;
            }
            // Reuse existing partitions, without making any modifications.
            InstallOption::Refresh(option) => {
                refresh_config(disks, option)?;
            }
            InstallOption::Recovery { option, password } => {
                recovery_config(disks, option, password)?;
            }
            // Reset the `disks` object and designate a disk to be wiped and installed.
            InstallOption::Erase { option, password } => {
                erase_config(disks, option, password)?;
            }
        }

        Ok(())
    }
}

fn alongside_config(
    disks: &mut Disks,
    option: &AlongsideOption,
    password: Option<String>,
    sectors: u64
) -> Result<(), InstallOptionError> {
    let mut tmp = Disks::new();
    mem::swap(&mut tmp, disks);

    let mut device = Disk::from_name(&option.device)
        .ok()
        .ok_or_else(|| InstallOptionError::DeviceNotFound {
            path: option.device.clone(),
        })?;

    let (mut start, end) = {
        let resize = device.get_partition_mut(option.partition)
            .ok_or_else(|| InstallOptionError::PartitionNotFoundByID {
                number: option.partition,
                device: option.device.clone()
            })?;

        let end = resize.end_sector;
        resize.shrink_to(sectors)?;
        (resize.end_sector + 1, end)
    };

    let (lvm, root_vg) = match generate_encryption(password)? {
        Some((enc, root)) => (Some((enc, root.clone())), Some(root)),
        None => (None, None),
    };

    let bootloader = Bootloader::detect();

    if bootloader == Bootloader::Efi {
        let mut create_esp = false;

        {
            let partitions = device.partitions.iter_mut();
            match partitions.filter(|p| p.is_esp_partition() && p.sectors() < 819200).next() {
                Some(esp) => esp.set_mount("/boot/efi".into()),
                None => create_esp = true
            }
        }

        if create_esp {
            // 500 MiB ESP partition.
            let esp_end = start + 1024_000;

            device.add_partition(
                PartitionBuilder::new(
                    start,
                    esp_end,
                    FileSystemType::Fat32
                ).flag(PartitionFlag::PED_PARTITION_ESP)
                .mount("/boot/efi".into())
            )?;

            start = esp_end;
        }

        // 4096 MiB recovery partition
        let recovery_end = start + 8388608
        device.add_partition(
            PartitionBuilder::new(start, recovery_end, FileSystemType::Fat32)
                .mount("/recovery")
                .name("recovery")
        )?;

        start = recovery_end;
    } else if lvm.is_some() {
        /// BIOS systems with an encrypted root must have a separate boot partition.
        let boot_end = start + 1024_000;

        device.add_partition(
            PartitionBuilder::new(start, boot_end, FileSystemType::Ext4)
                .partition_type(PartitionType::Primary)
                .flag(PartitionFlag::PED_PARTITION_BOOT)
                .mount("/boot".into())
        )?;

        start = boot_end;
    }

    // Configure optionally-encrypted root volume
    if let Some((enc, root_vg)) = lvm {
        device.add_partition(
            PartitionBuilder::new(start, end, FileSystemType::Lvm)
                .partition_type(PartitionType::Primary)
                .logical_volume(root_vg, Some(enc))
        )?;
    } else {
        let swap = device.get_sector(Sector::MegabyteFromEnd(4096));

        device.add_partition(
            PartitionBuilder::new(start, swap, FileSystemType::Ext4)
                .mount("/".into())
        ).and_then(|_| device.add_partition::new(swap, end, FileSystemType::Swap));
    }

    disks.add(device);
    disks.initialize_volume_groups()?;

    if let Some(root_vg) = root_vg {
        let lvm_device = disks
            .get_logical_device_mut(&root_vg)
            .ok_or(InstallOptionError::LogicalDeviceNotFound { vg: root_vg })?;

        let start = lvm_device.get_sector(Sector::Start);
        let swap = lvm_device.get_sector(Sector::MegabyteFromEnd(4096));
        let end = lvm_device.get_sector(Sector::End);

        lvm_device.add_partition(
            PartitionBuilder::new(start, swap, FileSystemType::Ext4)
                .name("root".into())
                .mount("/".into()),
        ).and_then(|_| {
            lvm_device.add_partition(PartitionBuilder::new(swap, end, FileSystemType::Swap)
        })?;
    }

    Ok(())
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
    let mut tmp = Disks::new();
    mem::swap(&mut tmp, disks);

    let (lvm, root_vg) = match generate_encryption(password)? {
        Some((enc, root)) => (Some((enc, root.clone())), Some(root)),
        None => (None, None),
    };

    let mut recovery_device: Disk = {
        let recovery_path: &Path = &misc::from_uuid(&option.root_uuid).unwrap();
        Disk::from_name(recovery_path)
            .ok()
            .ok_or(InstallOptionError::DeviceNotFound {
                path: recovery_path.to_path_buf(),
            })?
    };

    {
        let recovery_device = &mut recovery_device;
        let lvm_part: Option<PathBuf> = {
            let path: &str = &misc::from_uuid(&option.root_uuid)
                .expect("no uuid for recovery root")
                .file_name()
                .expect("path does not have file name")
                .to_owned()
                .into_string()
                .expect("path is not UTF-8");

            misc::resolve_slave(path).or_else(|| {
                // Attempt to find the LVM partition automatically.
                for part in recovery_device.get_partitions() {
                    if part
                        .filesystem
                        .as_ref()
                        .map_or(false, |&p| p == FileSystemType::Luks)
                    {
                        return Some(part.get_device_path().to_path_buf());
                    }
                }

                None
            })
        };

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
                .ok_or(InstallOptionError::PartitionNotFound { uuid: uuid.clone() })
                .map(|part| part.set_mount("/recovery".into()))?;
        }

        let (start, end);

        if let Some(part) = lvm_part {
            let id = {
                let part = recovery_device
                    .get_partitions()
                    .iter()
                    .find(|d| d.get_device_path() == part)
                    .ok_or(InstallOptionError::PartitionNotFound {
                        uuid: part.to_string_lossy().to_string(),
                    })?;

                start = part.start_sector;
                end = part.end_sector;
                part.number
            };

            recovery_device.remove_partition(id)?;
        } else {
            return Err(InstallOptionError::RecoveryNoLvm);
        }

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
    let mut tmp = Disks::new();
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
