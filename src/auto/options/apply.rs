use std::{fmt, fs::File, io::BufReader, mem};

use super::{
    super::super::*, AlongsideMethod, AlongsideOption, EraseOption, InstallOptionError,
    RecoveryOption, RefreshOption,
};
use disk_types::{FileSystem::*, SectorExt};

use crate::external::{generate_unique_id, remount_rw};
use crate::misc;
use partition_identity::PartitionID;
use proc_mounts::MountIter;

pub enum InstallOption<'a> {
    Alongside { option: &'a AlongsideOption, password: Option<String>, sectors: u64 },
    Refresh(&'a RefreshOption),
    Erase { option: &'a EraseOption, password: Option<String> },
    Recovery { option: &'a RecoveryOption, password: Option<String> },
    Upgrade(&'a RecoveryOption),
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
            InstallOption::Upgrade(ref option) => {
                write!(f, "InstallOption::UpgradeOption({:?})", option)
            }
            InstallOption::Recovery { .. } => write!(f, "InstallOption::RecoveryOption"),
            InstallOption::Erase { ref option, .. } => {
                write!(f, "InstallOption::EraseOption {{ option: {:?}, .. }}", option)
            }
        }
    }
}

fn set_mount_by_identity(
    disks: &mut Disks,
    id: &PartitionID,
    mount: &str,
) -> Result<(), InstallOptionError> {
    disks
        .get_partition_by_id_mut(id)
        .ok_or_else(|| InstallOptionError::PartitionIDNotFound { id: id.clone() })
        .map(|part| {
            part.set_mount(mount.into());
        })
}

fn generate_encryption(
    password: Option<String>,
    filesystem: FileSystem
) -> Result<Option<LuksEncryption>, InstallOptionError> {
    let value = match password {
        Some(pass) => {
            let encrypted_pv = generate_unique_id("cryptdata", &[])
                .map_err(|why| InstallOptionError::GenerateID { why })?;

            Some(LuksEncryption::new(encrypted_pv, Some(pass), None, filesystem))
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
                alongside_config(disks, option, password, sectors)
            }
            // Reuse existing partitions, without making any modifications.
            InstallOption::Refresh(option) => refresh_config(disks, option),
            // Perform a recovery install
            InstallOption::Recovery { option, password } => {
                recovery_config(disks, option, password)
            }
            // Reset the `disks` object and designate a disk to be wiped and installed.
            InstallOption::Erase { option, password } => erase_config(disks, option, password),
            InstallOption::Upgrade(option) => upgrade_config(disks, option),
        }
    }
}

fn alongside_config(
    disks: &mut Disks,
    option: &AlongsideOption,
    password: Option<String>,
    sectors: u64,
) -> Result<(), InstallOptionError> {
    let mut tmp = Disks::default();
    mem::swap(&mut tmp, disks);

    let mut device = Disk::from_name(&option.device)
        .ok()
        .ok_or_else(|| InstallOptionError::DeviceNotFound { path: option.device.clone() })?;

    let (mut start, end) = match option.method {
        AlongsideMethod::Shrink { partition, .. } => {
            let resize = device.get_partition_mut(partition).ok_or_else(|| {
                InstallOptionError::PartitionNotFoundByID {
                    number: partition,
                    device: option.device.clone(),
                }
            })?;

            let end = resize.end_sector;
            resize.shrink_to(sectors)?;
            (resize.end_sector + 1, end)
        }
        AlongsideMethod::Free(ref region) => (region.start + 1, region.end - 1),
    };

    let root_encryption = generate_encryption(password, Btrfs)?;
    let bootloader = Bootloader::detect();

    if bootloader == Bootloader::Efi {
        // NOTE: Logic that can enable re-using an existing EFI partition.
        // {
        //     let mut partitions = device.partitions.iter_mut();
        //     match partitions.find(|p| p.is_esp_partition() && p.get_sectors() > 819_200) {
        //         Some(esp) => esp.set_mount("/boot/efi".into()),
        //         None => create_esp = true
        //     }
        // }

        let esp_end = start + DEFAULT_ESP_SECTORS;

        device.add_partition(
            PartitionBuilder::new(start, esp_end, Fat32)
                .flag(PartitionFlag::PED_PARTITION_ESP)
                .mount("/boot/efi".into()),
        )?;

        start = esp_end;

        let recovery_end = start + DEFAULT_RECOVER_SECTORS;
        device.add_partition(
            PartitionBuilder::new(start, recovery_end, Fat32)
                .mount("/recovery".into())
                .name("recovery".into()),
        )?;

        start = recovery_end;
    } else if root_encryption.is_some() {
        // BIOS systems with an encrypted root must have a separate boot partition.
        let boot_end = start + DEFAULT_ESP_SECTORS;

        device.add_partition(
            PartitionBuilder::new(start, boot_end, Ext4)
                .partition_type(PartitionType::Primary)
                .flag(PartitionFlag::PED_PARTITION_BOOT)
                .mount("/boot".into()),
        )?;

        start = boot_end;
    }

    // Configure optionally-encrypted root volume
    if let Some(encryption) = root_encryption {
        device.add_partition(
            PartitionBuilder::new(start, end, Luks)
                .partition_type(PartitionType::Primary)
                .encryption(encryption)
                .name("cryptdata".into())
                .subvolume("/", "@root")
                .subvolume("/home", "@home")
        )?;
    } else {
        let swap = end - DEFAULT_SWAP_SECTORS;

        // Only create a new unencrypted swap partition if a swap partition does not already exist.
        let end = if !device.get_partitions().iter().any(|p| p.filesystem == Some(Swap)) {
            device.add_partition(PartitionBuilder::new(swap, end, Swap))?;
            swap
        } else {
            end
        };

        device.add_partition(
            PartitionBuilder::new(start, end, Btrfs)
                .name("Pop".into())
                .subvolume("/", "@root")
                .subvolume("/home", "@home")
        )?;
    }

    disks.add(device);
    disks.initialize_volume_groups()?;

    Ok(())
}

fn upgrade_config(disks: &mut Disks, option: &RecoveryOption) -> Result<(), InstallOptionError> {
    info!("applying upgrade config");
    set_mount_by_identity(disks, &PartitionID::new_uuid(option.root_uuid.clone()), "/")?;

    if let Some(ref efi) = option.efi_uuid {
        let efi = efi.parse::<PartitionID>().unwrap();
        set_mount_by_identity(disks, &efi, "/boot/efi")?;
    }

    let recovery = option.recovery_uuid.parse::<PartitionID>().unwrap();
    mount_recovery_partid(disks, &recovery)?;

    Ok(())
}

/// Apply a `refresh` config to `disks`.
fn refresh_config(disks: &mut Disks, option: &RefreshOption) -> Result<(), InstallOptionError> {
    info!("applying refresh install config: {:#?}", option);
    info!("disk configuration: {:#?}", disks);

    {
        let root = &option.root;
        let subvolume = root.options.split(',')
            .find_map(|o| o.strip_prefix("subvol="));

        if let Some(subvol) = subvolume {
            debug!("setting root subvol");
            disks.set_subvolume(&root.source, subvol, "/");
        } else {
            debug!("setting root");
            set_mount_by_identity(disks, &root.source, "/")?;
        }
    }


    if let Some(ref home) = option.home_part {
        debug!("home = {}", home.options);
        let subvolume = home.options.split(',')
            .find_map(|o| o.strip_prefix("subvol="));

        if let Some(subvol) = subvolume {
            debug!("setting home subvol");
            disks.set_subvolume(&home.source, subvol, "/home");
        } else {
            debug!("setting home");
            set_mount_by_identity(disks, &home.source, "/home")?;
        }
    }

    if let Some(ref efi) = option.efi_part {
        set_mount_by_identity(disks, &efi.source, "/boot/efi")?;
    } else if Bootloader::detect() == Bootloader::Efi {
        return Err(InstallOptionError::RefreshWithoutEFI);
    }

    if let Some(ref recovery) = option.recovery_part {
        mount_recovery_partid(disks, &recovery.source)?;
    }

    Ok(())
}

fn mount_recovery_partid(
    disks: &mut Disks,
    recovery: &PartitionID,
) -> Result<(), InstallOptionError> {
    if let Some(path) = recovery.get_device_path() {
        let recovery_is_cdrom = MountIter::<BufReader<File>>::source_mounted_at(path, "/cdrom")
            .map_err(|why| InstallOptionError::ProcMounts { why })?;

        if recovery_is_cdrom {
            info!("remounting /cdrom as rewriteable");
            remount_rw("/cdrom").map_err(InstallOptionError::RemountCdrom)?;
        }

        set_mount_by_identity(disks, recovery, "/recovery")?;
    }

    Ok(())
}

/// Apply a `recovery mode` config to `disks`.
fn recovery_config(
    disks: &mut Disks,
    option: &RecoveryOption,
    password: Option<String>,
) -> Result<(), InstallOptionError> {
    let mut tmp = Disks::default();
    mem::swap(&mut tmp, disks);

    let luks = generate_encryption(password, Btrfs)?;

    let mut recovery_device: Disk = {
        let mut recovery_path = option.parse_recovery_id().get_device_path().ok_or_else(|| {
            InstallOptionError::PartitionNotFound { uuid: option.recovery_uuid.clone() }
        })?;

        if let Some(physical) =
            misc::resolve_to_physical(recovery_path.file_name().unwrap().to_str().unwrap())
        {
            recovery_path = physical;
        }

        if let Some(parent) =
            misc::resolve_parent(recovery_path.file_name().unwrap().to_str().unwrap())
        {
            recovery_path = parent;
        }

        info!("recovery disk found at {:?}", recovery_path);
        Disk::from_name(&recovery_path)
            .ok()
            .ok_or(InstallOptionError::DeviceNotFound { path: recovery_path })?
    };

    {
        let recovery_device = &mut recovery_device;
        let lvm_part: Option<PathBuf> =
            option.luks_uuid.clone().and_then(|uuid| PartitionID::new_uuid(uuid).get_device_path());

        if let Some(ref uuid) = option.efi_uuid {
            let path =
                option.parse_efi_id().unwrap().get_device_path().expect("no uuid for efi part");
            recovery_device
                .get_partitions_mut()
                .iter_mut()
                .find(|d| d.get_device_path() == path)
                .ok_or(InstallOptionError::PartitionNotFound { uuid: uuid.clone() })
                .map(|part| part.set_mount("/boot/efi".into()))?;
        }

        {
            let uuid = &option.recovery_uuid;
            let path =
                &option.parse_recovery_id().get_device_path().expect("no uuid for recovery part");
            recovery_device
                .get_partitions_mut()
                .iter_mut()
                .find(|d| d.get_device_path() == path)
                .ok_or(InstallOptionError::PartitionNotFound { uuid: uuid.clone() })?
                .set_mount("/recovery".into());
        }

        let (start, end);

        let root_path = if let Some(mut part) = lvm_part {
            if let Some(physical) =
                misc::resolve_to_physical(part.file_name().unwrap().to_str().unwrap())
            {
                part = physical;
            }

            part
        } else {
            PartitionID::new_uuid(option.root_uuid.clone()).get_device_path().ok_or_else(|| {
                InstallOptionError::PartitionNotFound { uuid: option.root_uuid.clone() }
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

        if let Some(enc) = luks {
            recovery_device.add_partition(
                PartitionBuilder::new(start, end, Luks)
                    .encryption(enc)
                    .name("cryptdata".into())
                    .subvolume("/", "@root")
                    .subvolume("/home", "@home"),
            )?;
        } else {
            recovery_device
                .add_partition(
                    PartitionBuilder::new(start, end, Btrfs)
                        .name("Pop".into())
                        .subvolume("/", "@root")
                        .subvolume("/home", "@home")
                )?;
        }
    }

    disks.add(recovery_device);
    disks.initialize_volume_groups()?;

    Ok(())
}

/// Apply an "erase and install" configuration to `disks`;
fn erase_config(
    disks: &mut Disks,
    option: &EraseOption,
    password: Option<String>,
) -> Result<(), InstallOptionError> {
    let mut tmp = Disks::default();
    mem::swap(&mut tmp, disks);

    let bootloader = Bootloader::detect();

    let start_sector = Sector::Start;
    let boot_sector = Sector::Unit(DEFAULT_ESP_SECTORS);
    let recovery_sector = Sector::Unit(DEFAULT_ESP_SECTORS + DEFAULT_RECOVER_SECTORS);
    let swap_sector = Sector::UnitFromEnd(DEFAULT_SWAP_SECTORS);
    let end_sector = Sector::End;

    let luks = generate_encryption(password, Btrfs)?;

    {
        let mut device = Disk::from_name(&option.device)
            .ok()
            .ok_or(InstallOptionError::DeviceNotFound { path: option.device.clone() })?;

        let result = match bootloader {
            Bootloader::Efi => {
                device
                    .mklabel(PartitionTable::Gpt)
                    // Configure ESP partition
                    .and_then(|_| {
                        let start = device.get_sector(start_sector);
                        let end = device.get_sector(boot_sector);
                        device.add_partition(
                            PartitionBuilder::new(start, end, Fat32)
                                .partition_type(PartitionType::Primary)
                                .flag(PartitionFlag::PED_PARTITION_ESP)
                                .mount("/boot/efi".into()),
                        )
                    })
                    // Configure recovery partition
                    .and_then(|_| {
                        let start = device.get_sector(boot_sector);
                        let end = device.get_sector(recovery_sector);
                        device.add_partition(
                            PartitionBuilder::new(start, end, Fat32)
                                .name("recovery".into())
                                .mount("/recovery".into()),
                        )
                    })
                    .map(|_| (device.get_sector(recovery_sector), device.get_sector(swap_sector)))
            }
            Bootloader::Bios => {
                device
                    .mklabel(PartitionTable::Msdos)
                    // This is used to ensure LVM installs will work with BIOS
                    .and_then(|_| {
                        if luks.is_some() {
                            let start = device.get_sector(start_sector);
                            let end = device.get_sector(boot_sector);
                            device
                                .add_partition(
                                    PartitionBuilder::new(start, end, Ext4)
                                        .partition_type(PartitionType::Primary)
                                        .flag(PartitionFlag::PED_PARTITION_BOOT)
                                        .mount("/boot".into()),
                                )
                                .map(|_| (boot_sector, swap_sector))
                        } else {
                            Ok((start_sector, swap_sector))
                        }
                    })
                    .map(|(start, end)| (device.get_sector(start), device.get_sector(end)))
            }
        };

        // Configure optionally-encrypted root volume
        result
            .and_then(|(start, end)| {
                device.add_partition(if let Some(enc) = luks {
                    PartitionBuilder::new(start, end, Luks)
                        .partition_type(PartitionType::Primary)
                        .encryption(enc)
                        .name("cryptdata".into())
                        .subvolume("/", "@root")
                        .subvolume("/home", "@home")
                } else {
                    PartitionBuilder::new(start, end, Btrfs)
                        .name("Pop".into())
                        .subvolume("/", "@root")
                        .subvolume("/home", "@home")
                })
            })
            // Configure swap partition
            .and_then(|_| {
                let start = device.get_sector(swap_sector);
                let end = device.get_sector(end_sector);
                device.add_partition(PartitionBuilder::new(start, end, Swap))
            })?;

        disks.add(device);
    }

    disks.initialize_volume_groups()?;

    dbg!(disks);

    Ok(())
}
