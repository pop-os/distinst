//! Detect whether a Linux system is in EFI or BIOS mode.
//!
//! ```rust,no_run
//! use distinst_bootloader::Bootloader;
//!
//! match Bootloader::detect() {
//!     Bootloader::Efi => println!("System is in EFI mode"),
//!     Bootloader::Bios => println!("System is in BIOS mode")
//! }
//! ```

use std::path::Path;
use std::sync::atomic::{ATOMIC_USIZE_INIT, AtomicUsize, Ordering};

/// Force the installation to perform either a BIOS or EFI installation.
pub static FORCE_BOOTLOADER: AtomicUsize = ATOMIC_USIZE_INIT;

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
