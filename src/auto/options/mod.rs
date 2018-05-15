//! Generate a list of installation options based on what already exists on the disk.

mod apply;

pub use self::apply::*;

use std::fmt;
use std::path::PathBuf;

use super::super::*;
use misc::{get_uuid, from_uuid};

#[derive(Debug)]
pub struct InstallOptions {
    pub refresh_options: Vec<RefreshOption>,
    pub erase_options: Vec<EraseOption>,
    // pub multiboot_options: Vec<MultiBootOption>,
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
                            OS::Linux { info, home, efi, recovery } => {
                                refresh_options.push(RefreshOption {
                                    os_name: info.pretty_name,
                                    os_version: info.version,
                                    root_part: get_uuid(part.get_device_path())
                                        .expect("root device did not have uuid"),
                                    home_part: home,
                                    efi_part: efi,
                                    recovery_part: recovery,
                                })
                            }
                            _ => ()
                        }
                        None => ()
                    }
                }
            };

            for device in disks.get_physical_devices() {
                if !Path::new("/cdrom/recovery.conf").exists()
                    && (device.contains_mount("/") || device.contains_mount("/cdrom"))
                {
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
                        let mut flags = if device.is_removable() { IS_REMOVABLE } else { 0 };
                        flags += if device.is_rotational() { IS_ROTATIONAL } else { 0 };
                        flags += if sectors >= required_space || required_space == 0 {
                            MEETS_REQUIREMENTS
                        } else {
                            0
                        };
                        flags
                    }
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

        InstallOptions { erase_options, refresh_options }
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
    GenerateID { why: io::Error }
}

impl From<DiskError> for InstallOptionError {
    fn from(why: DiskError) -> InstallOptionError {
        InstallOptionError::DiskError { why }
    }
}

pub const IS_ROTATIONAL: u8 = 1;
pub const IS_REMOVABLE: u8 = 2;
pub const MEETS_REQUIREMENTS: u8 = 3;

#[derive(Debug)]
pub struct EraseOption {
    pub device: PathBuf,
    pub model: String,
    pub sectors: u64,
    pub flags: u8
}

impl fmt::Display for EraseOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Erase and Install to {} ({})", self.model, self.device.display())
    }
}

impl EraseOption {
    pub fn is_rotational(&self) -> bool {
        self.flags & IS_ROTATIONAL != 0
    }

    pub fn is_removable(&self) -> bool {
        self.flags & IS_REMOVABLE != 0
    }

    pub fn meets_requirements(&self) -> bool {
        self.flags & MEETS_REQUIREMENTS != 0
    }

    pub fn get_linux_icon(&self) -> &'static str {
        const BOTH: u8 = IS_ROTATIONAL | IS_REMOVABLE;
        match self.flags & BOTH {
            BOTH => "drive-harddisk-usb",
            IS_ROTATIONAL => "drive-harddisk-scsi",
            IS_REMOVABLE => "drive-removable-media-usb",
            0 => "drive-harddisk-solidstate",
            _ => unreachable!("get_linux_icon(): branch not handled")
        }
    }
}

#[derive(Debug)]
pub struct RefreshOption {
    pub os_name: String,
    pub os_version: String,
    pub root_part: String,
    pub home_part: Option<String>,
    pub efi_part: Option<String>,
    pub recovery_part: Option<String>
}

impl fmt::Display for RefreshOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let root_part: String = match from_uuid(&self.root_part) {
            Some(uuid) => uuid.to_string_lossy().into(),
            None => "None".into()
        };

        write!(f, "Refresh {} on {}", self.os_name, root_part)
    }
}
