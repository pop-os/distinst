pub mod mount;

pub(crate) mod config;
mod error;
mod mounts;
pub(crate) mod operations;
mod serial;
mod swaps;

pub use self::config::*;
pub use self::error::{DecryptionError, DiskError, PartitionError, PartitionSizeError};
pub(crate) use self::mounts::{Mounts, MOUNTS};
pub(crate) use self::swaps::{Swaps, SWAPS};
pub use libparted::PartitionFlag;
use libparted::{Device, DiskType as PedDiskType};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::FORCE_BOOTLOADER;

/// Bootloader type
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Bootloader {
    Bios,
    Efi,
}

impl Bootloader {
    /// Detects whether the system is running from EFI.
    pub fn detect() -> Bootloader {
        match FORCE_BOOTLOADER.load(Ordering::SeqCst) {
            1 => {
                return Bootloader::Bios;
            }
            2 => {
                return Bootloader::Efi;
            }
            _ => ()
        }

        if Path::new("/sys/firmware/efi").is_dir() {
            Bootloader::Efi
        } else {
            Bootloader::Bios
        }
    }
}
