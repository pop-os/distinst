//! A crate for installing Linux distributions from a live squashfs.
//!
//! > Currently, only Pop!\_OS and Ubuntu are supported by this installer.

#![allow(unknown_lints)]

pub extern crate disk_types;
pub extern crate distinst_bootloader as bootloader;
pub extern crate distinst_chroot as chroot;
pub extern crate distinst_disks as disks;
pub extern crate distinst_external_commands as external;
pub extern crate distinst_hardware_support as hardware_support;
pub extern crate distinst_locale_support as locale;
pub extern crate distinst_squashfs as squashfs;
pub extern crate hostname_validator as hostname;
pub extern crate os_detect;
pub extern crate os_release;
pub extern crate partition_identity;
pub extern crate proc_mounts;
pub extern crate sys_mount;

extern crate anyhow;
extern crate apt_cli_wrappers;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate cascade;
extern crate err_derive;
#[macro_use]
extern crate derive_more;
extern crate dirs;
pub extern crate distinst_timezones as timezones;
extern crate distinst_utils as misc;
extern crate envfile;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate fern;
#[macro_use]
extern crate fomat_macros;
extern crate fstab_generate;
extern crate itertools;
extern crate libc;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate logind_dbus;
extern crate rayon;
extern crate systemd_boot_conf;
extern crate tempdir;

pub use crate::bootloader::*;
pub use disk_types::*;
pub use crate::disks::*;
pub use crate::misc::device_layout_hash;
pub use crate::upgrade::*;

pub use self::installer::RecoveryEnv;

mod distribution;
mod installer;
mod logging;
mod upgrade;

pub mod auto;
pub(crate) mod errors;

/// Useful DBus interfaces for installers to implement.
pub mod dbus_interfaces {
    pub use logind_dbus::*;
}

use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use anyhow::Context;
use crate::external::dmlist;
use partition_identity::PartitionID;
use sys_mount::*;
use systemd_boot_conf::SystemdBootConf;

pub use self::{installer::*, logging::log};

/// When set to true, this will stop the installation process.
pub static KILL_SWITCH: AtomicBool = AtomicBool::new(false);

pub use crate::bootloader::FORCE_BOOTLOADER;

/// Exits before the unsquashfs step
pub static PARTITIONING_TEST: AtomicBool = AtomicBool::new(false);

/// Even if the system is EFI, the efivars directory will not be mounted in the chroot.
pub static NO_EFI_VARIABLES: AtomicBool = AtomicBool::new(false);

/// 500 MiB EFI partition
pub const DEFAULT_ESP_SECTORS: u64 = 1_024_000;

/// 4096 MiB recovery partition
pub const DEFAULT_RECOVER_SECTORS: u64 = 8_388_608;

pub const DEFAULT_SWAP_SECTORS: u64 = DEFAULT_RECOVER_SECTORS;

/// Checks if the given name already exists as a device in the device map list.
pub fn device_map_exists(name: &str) -> bool {
    dmlist().ok().map_or(false, |list| list.contains(&name.into()))
}

/// Gets the minimum number of sectors required. The input should be in sectors, not bytes.
///
/// The number of sectors required is calculated through:
///
/// - The value in `/cdrom/casper/filesystem.size`
/// - The size of a default boot / esp partition
/// - The size of a default swap partition
/// - The size of a default recovery partition.
///
/// The input parameter will undergo a max comparison to the estimated minimum requirement.
pub fn minimum_disk_size(default: u64) -> u64 {
    let casper = std::fs::read_to_string("/cdrom/casper/filesystem.size")
        .ok()
        // File contains a number in bytes
        .and_then(|size| size.trim().parse::<u64>().ok())
        // Convert bytes read into sectors required + 1
        .map_or(default, |size| ((size / 512) + 1).max(default));

    // EFI installs will contain an EFI partition with a recovery partition.
    let bootloader = if Bootloader::detect() == Bootloader::Efi {
        DEFAULT_ESP_SECTORS + DEFAULT_RECOVER_SECTORS
    } else {
        0
    };

    casper + bootloader + DEFAULT_SWAP_SECTORS
}

pub fn unset_mode() -> anyhow::Result<()> {
    let mut conf = RecoveryEnv::new().context("failed to read recovery.conf")?;

    if conf.get("MODE") == Some("refresh") {
        let efi_id = conf.get("EFI_UUID").context("EFI_UUID is not set")?;
        let prev_boot = conf.get("PREV_BOOT").context("PREV_BOOT is not set")?;

        let target_dir = Path::new("target");

        let _mount = mount_efi(efi_id, target_dir)?;

        let mut boot_loader =
            SystemdBootConf::new(target_dir).context("failed to open boot loader conf")?;

        boot_loader.loader_conf.default = Some(prev_boot.into());
        boot_loader.overwrite_loader_conf().context("failed to overwrite boot loader conf")?;

        crate::external::remount_rw("/cdrom")
            .context("failed to remount /cdrom with write permissions")?;
        conf.remove("MODE");
        conf.remove("PREV_BOOT");
        conf.write().context("failed to write updated boot loader conf")?;
    }

    Ok(())
}

fn mount_efi(efi_id: &str, target_dir: &Path) -> anyhow::Result<UnmountDrop<Mount>> {
    let efi_pid = if efi_id.starts_with("PARTUUID=") {
        PartitionID::new_partuuid(efi_id[9..].to_owned())
    } else {
        PartitionID::new_uuid(efi_id.to_owned())
    };

    let efi_path =
        efi_pid.get_device_path().context("failed to get device path from EFI partition")?;

    if !target_dir.exists() {
        std::fs::create_dir(target_dir)
            .context("failed to create target directory for EFI mount")?;
    }

    Mount::builder()
        .fstype("vfat")
        .mount_autodrop(&efi_path, target_dir, UnmountFlags::DETACH)
        .context("failed to mount EFI device")
}
