//! A crate for installing Ubuntu distributions from a live squashfs

#![allow(unknown_lints)]

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate cascade;
extern crate dirs;
#[macro_use]
extern crate derive_new;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate fern;
extern crate itertools;
#[macro_use]
extern crate lazy_static;
extern crate libc;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate gettextrs;
extern crate iso3166_1;
extern crate isolang;
extern crate rand;
extern crate rayon;
extern crate raw_cpuid;
extern crate tempdir;
#[macro_use]
extern crate serde_derive;
extern crate serde_xml_rs;

use process::external::{blockdev, cryptsetup_close, dmlist, encrypted_devices, pvs, vgdeactivate, CloseBy};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, ATOMIC_BOOL_INIT, ATOMIC_USIZE_INIT};

pub use process::{Chroot, Command};
pub use mnt::{BIND, Mount, Mounts};
pub use disk::{
    generate_unique_id, Bootloader, DecryptionError, Disk, DiskError, DiskExt, Disks,
    FileSystemType, LvmDevice, LvmEncryption, PartitionBuilder, PartitionError, PartitionFlag,
    PartitionInfo, PartitionTable, PartitionType, Sector, OS,
};
pub use misc::device_layout_hash;

mod disk;
mod distribution;
mod envfile;
mod hardware_support;
mod installer;
mod logging;
mod misc;
mod mnt;
mod squashfs;

pub mod auto;
pub mod hostname;
pub mod locale;
pub mod os_release;
pub mod process;

pub use self::installer::*;
pub use self::logging::log;

/// When set to true, this will stop the installation process.
pub static KILL_SWITCH: AtomicBool = ATOMIC_BOOL_INIT;

/// Force the installation to perform either a BIOS or EFI installation.
pub static FORCE_BOOTLOADER: AtomicUsize = ATOMIC_USIZE_INIT;

/// Exits before the unsquashfs step
pub static PARTITIONING_TEST: AtomicBool = ATOMIC_BOOL_INIT;

/// Even if the system is EFI, the efivars directory will not be mounted in the chroot.
pub static NO_EFI_VARIABLES: AtomicBool = ATOMIC_BOOL_INIT;

pub const DEFAULT_ESP_SECTORS: u64 = 1_024_000;
pub const DEFAULT_RECOVER_SECTORS: u64 = 8_388_608;
pub const DEFAULT_SWAP_SECTORS: u64 = DEFAULT_RECOVER_SECTORS;

pub fn deactivate_logical_devices() -> io::Result<()> {
    for luks_pv in encrypted_devices()? {
        info!("deactivating encrypted device named {}", luks_pv);
        if let Some(vg) = pvs()?.get(&PathBuf::from(["/dev/mapper/", &luks_pv].concat())) {
            match *vg {
                Some(ref vg) => {
                    vgdeactivate(vg).and_then(|_| cryptsetup_close(CloseBy::Name(&luks_pv)))?;
                },
                None => {
                    cryptsetup_close(CloseBy::Name(&luks_pv))?;
                },
            }
        }
    }

    Ok(())
}

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
