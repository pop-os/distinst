//! Generate a list of installation options based on what already exists on the disk.

mod apply;

pub use self::apply::*;

use std::fmt;
use std::path::PathBuf;

use super::super::*;
use misc::{from_uuid, get_uuid};

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
                if !Path::new("/cdrom/recovery.conf").exists()
                    && (device.contains_mount("/") || device.contains_mount("/cdrom"))
                {
                    continue;
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
                        flags += if device.is_rotational() {
                            IS_ROTATIONAL
                        } else {
                            0
                        };
                        flags += if sectors >= required_space || required_space == 0 {
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

const RECOVERY_CONF: &'static str = "/cdrom/recovery.conf";

fn detect_recovery() -> Option<RecoveryOption> {
    let recovery_path = Path::new(RECOVERY_CONF);
    if recovery_path.exists() {
        let env = match EnvFile::new(recovery_path) {
            Ok(env) => env,
            Err(why) => {
                warn!(
                    "libdistinst: unable to read recovery configuration: {}",
                    why
                );
                return None;
            }
        };

        return Some(RecoveryOption {
            hostname:      env.get("HOSTNAME")?.to_owned(),
            language:      env.get("LANG")?.to_owned(),
            kbd_layout:    env.get("KBD_LAYOUT")?.to_owned(),
            kbd_model:     env.get("KBD_MODEL").map(|x| x.to_owned()),
            kbd_variant:   env.get("KBD_VARIANT").map(|x| x.to_owned()),
            efi_uuid:      env.get("EFI_UUID").map(|x| x.to_owned()),
            recovery_uuid: env.get("RECOVERY_UUID")?.to_owned(),
            root_uuid:     env.get("ROOT_UUID")?.to_owned(),
            oem_mode:      env.get("OEM_MODE").map_or(false, |oem| oem == "1"),
        });
    }

    None
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

#[derive(Debug)]
pub struct RecoveryOption {
    pub efi_uuid:      Option<String>,
    pub hostname:      String,
    pub kbd_layout:    String,
    pub kbd_model:     Option<String>,
    pub kbd_variant:   Option<String>,
    pub language:      String,
    pub oem_mode:      bool,
    pub recovery_uuid: String,
    pub root_uuid:     String,
}

pub const IS_ROTATIONAL: u8 = 1;
pub const IS_REMOVABLE: u8 = 2;
pub const MEETS_REQUIREMENTS: u8 = 3;

#[derive(Debug)]
pub struct EraseOption {
    pub device:  PathBuf,
    pub model:   String,
    pub sectors: u64,
    pub flags:   u8,
}

impl fmt::Display for EraseOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Erase and Install to {} ({})",
            self.model,
            self.device.display()
        )
    }
}

impl EraseOption {
    pub fn is_rotational(&self) -> bool { self.flags & IS_ROTATIONAL != 0 }

    pub fn is_removable(&self) -> bool { self.flags & IS_REMOVABLE != 0 }

    pub fn meets_requirements(&self) -> bool { self.flags & MEETS_REQUIREMENTS != 0 }

    pub fn get_linux_icon(&self) -> &'static str {
        const BOTH: u8 = IS_ROTATIONAL | IS_REMOVABLE;
        match self.flags & BOTH {
            BOTH => "drive-harddisk-usb",
            IS_ROTATIONAL => "drive-harddisk-scsi",
            IS_REMOVABLE => "drive-removable-media-usb",
            0 => "drive-harddisk-solidstate",
            _ => unreachable!("get_linux_icon(): branch not handled"),
        }
    }
}

#[derive(Debug)]
pub struct RefreshOption {
    pub os_name:        String,
    pub os_pretty_name: String,
    pub os_version:     String,
    pub root_part:      String,
    pub home_part:      Option<String>,
    pub efi_part:       Option<String>,
    pub recovery_part:  Option<String>,
}

impl fmt::Display for RefreshOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let root_part: String = match from_uuid(&self.root_part) {
            Some(uuid) => uuid.to_string_lossy().into(),
            None => "None".into(),
        };

        write!(f, "Refresh {} on {}", self.os_name, root_part)
    }
}
