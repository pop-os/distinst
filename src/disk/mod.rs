pub(crate) mod external;
pub mod mount;

mod config;
mod error;
mod mounts;
mod operations;
mod serial;
mod swaps;

pub use self::config::*;
pub use self::error::{DiskError, PartitionSizeError};
pub use self::swaps::Swaps;
pub use libparted::PartitionFlag;

use self::mounts::Mounts;
use libparted::{Device, DiskType as PedDiskType};
use std::path::{Path, PathBuf};

/// Bootloader type
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Bootloader {
    Bios,
    Efi,
}

impl Bootloader {
    /// Detects whether the system is running from EFI.
    pub fn detect() -> Bootloader {
        if Path::new("/sys/firmware/efi").is_dir() {
            Bootloader::Efi
        } else {
            Bootloader::Bios
        }
    }
}
