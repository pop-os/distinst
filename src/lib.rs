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

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate cascade;
extern crate dirs;
extern crate distinst_utils as misc;
pub extern crate distinst_timezones as timezones;
extern crate envfile;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate fstab_generate;
extern crate fern;
extern crate itertools;
extern crate libc;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate logind_dbus;
extern crate rayon;
extern crate tempdir;

pub use disks::*;
pub use disk_types::*;
pub use bootloader::*;
pub use misc::device_layout_hash;

mod distribution;
mod installer;
mod logging;
pub mod auto;
pub(crate) mod errors;

/// Useful DBus interfaces for installers to implement.
pub mod dbus_interfaces {
    pub use logind_dbus::*;
}

use external::dmlist;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, ATOMIC_BOOL_INIT};

pub use self::installer::*;
pub use self::logging::log;

/// When set to true, this will stop the installation process.
pub static KILL_SWITCH: AtomicBool = ATOMIC_BOOL_INIT;

pub use bootloader::FORCE_BOOTLOADER;

/// Exits before the unsquashfs step
pub static PARTITIONING_TEST: AtomicBool = ATOMIC_BOOL_INIT;

/// Even if the system is EFI, the efivars directory will not be mounted in the chroot.
pub static NO_EFI_VARIABLES: AtomicBool = ATOMIC_BOOL_INIT;

pub const DEFAULT_ESP_SECTORS: u64 = 1_024_000;
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
    let casper_size = misc::open("/cdrom/casper/filesystem.size")
        .ok()
        .and_then(|mut file| {
            let capacity = file.metadata().ok().map_or(0, |m| m.len());
            let mut buffer = String::with_capacity(capacity as usize);
            file.read_to_string(&mut buffer)
                .ok()
                .and_then(|_| buffer[..buffer.len() - 1].parse::<u64>().ok())
        })
        // Convert the number of bytes read into sectors required + 1
        .map(|bytes| (bytes / 512) + 1)
        .map_or(default, |size| size.max(default));

    casper_size + DEFAULT_ESP_SECTORS + DEFAULT_RECOVER_SECTORS + DEFAULT_SWAP_SECTORS
}
