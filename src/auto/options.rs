//! Generate a list of installation options based on what already exists on the disk.

use std::fmt;
use std::mem;
use std::path::PathBuf;

use super::super::*;
use misc::{get_uuid, from_uuid};

#[derive(Debug)]
pub struct InstallOptions {
    pub refresh_options: Vec<RefreshOption>,
    pub erase_options: Vec<EraseAndInstall>,
}

#[derive(Debug)]
pub enum InstallOptionError {
    PartitionNotFound { uuid: String },
    DeviceNotFound { path: PathBuf },
    LogicalDeviceNotFound { vg: String },
    DiskError { why: DiskError },
    GenerateID { why: io::Error }
}

impl From<DiskError> for InstallOptionError {
    fn from(why: DiskError) -> InstallOptionError {
        InstallOptionError::DiskError { why }
    }
}

#[derive(Debug)]
pub struct EraseAndInstall {
    device: PathBuf,
    sectors: u64
}

impl fmt::Display for EraseAndInstall {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Erase and Install to {}", self.device.display())
    }
}

#[derive(Debug)]
pub enum InstallOption<'a> {
    RefreshOption(&'a RefreshOption),
    EraseAndInstall {
        option: &'a EraseAndInstall,
        password: Option<String>,
    }
}

#[derive(Debug)]
pub struct RefreshOption {
    os_name: String,
    os_version: String,
    root_part: String,
    root_sectors: u64,
    home_part: Option<String>,
    efi_part: Option<String>
}

impl fmt::Display for RefreshOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let root_part = match from_uuid(&self.root_part) {
            Some(uuid) => uuid,
            None => "None".into()
        };

        let mut msg = format!("Refresh {} on {:?}", self.os_name, root_part);
        if let Some(ref efi) = self.efi_part {
            msg.push_str(&[", /boot/efi on UUID=", efi].concat());
        }

        if let Some(ref home) = self.home_part {
            msg.push_str(&[", /home on UUID=", home].concat());
        }

        write!(f, "{}", msg)
    }
}

impl InstallOptions {
    /// Detects existing installations, and suggests new ones.
    ///
    /// Note that encrypted partitions will need to be decrypted within the `disks` object
    /// in order for the installed operating systems on them to be detected and reinstalled to.
    pub fn new(disks: &Disks, required_space: u64) -> InstallOptions {
        let mut erase_options = Vec::new();
        let mut refresh_options = Vec::new();

        {
            let erase_options = &mut erase_options;
            let refresh_options = &mut refresh_options;
            
            let mut check_partition = |part: &PartitionInfo| {
                if part.is_linux_compatible() {
                    match part.probe_os() {
                        Some(os) => match os {
                            OS::Linux { info, home, efi } => {
                                refresh_options.push(RefreshOption {
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
            };

            for device in disks.get_physical_devices() {
                if device.get_sectors() >= required_space || required_space == 0 {
                    erase_options.push(EraseAndInstall {
                        device: device.get_device_path().to_path_buf(),
                        sectors: device.get_sectors()
                    })
                }

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

        InstallOptions { erase_options, refresh_options }
    }

    /// Applies a given installation option to the `disks` object.
    ///
    /// If the option is to erase and install, the `disks` object will be replaced with a new one.
    pub fn apply(&self, disks: &mut Disks, option: InstallOption) -> Result<(), InstallOptionError> {
        let bootloader = Bootloader::detect();

        match option {
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
            },
            // Reset the `disks` object and designate a disk to be wiped and installed.
            InstallOption::EraseAndInstall { option, password } => {
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
