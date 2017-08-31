//! A crate for installing Ubuntu distributions from a live squashfs

use std::{io, path};

use disk::Disk;
pub use chroot::Chroot;
pub use mount::{Mount, MountKind};

#[doc(hidden)]
pub use c::*;

mod c;
mod chroot;
mod disk;
mod mount;

/// Bootloader type
#[derive(Copy, Clone, Debug)]
pub enum Bootloader {
    Bios,
    Efi,
}

impl Bootloader {
    pub fn detect() -> Bootloader {
        if path::Path::new("/sys/firmware/efi").is_dir() {
            Bootloader::Efi
        } else {
            Bootloader::Bios
        }
    }
}

/// Installation step
#[derive(Copy, Clone, Debug)]
pub enum Step {
    Partition,
    Format,
    Extract,
    Bootloader,
}

/// Installer configuration
#[derive(Debug)]
pub struct Config {
    squashfs: String,
    drive: String,
}

/// Installer error
#[derive(Debug)]
pub struct Error {
    pub step: Step,
    pub err: io::Error,
}

/// Installer status
#[derive(Copy, Clone, Debug)]
pub struct Status {
    pub step: Step,
    pub percent: i32,
}

/// An installer object
pub struct Installer {
    error_cb: Option<Box<FnMut(&Error)>>,
    status_cb: Option<Box<FnMut(&Status)>>,
}

impl Installer {
    /// Create a new installer object
    ///
    /// ```
    /// use distinst::Installer;
    /// let installer = Installer::new();
    /// ```
    pub fn new() -> Installer {
        Installer {
            error_cb: None,
            status_cb: None,
        }
    }

    /// Send an error message
    ///
    /// ```
    /// use std::io;
    /// use distinst::{Installer, Error, Step};
    /// let mut installer = Installer::new();
    /// installer.emit_error(&Error {
    ///     step: Step::Extract,
    ///     err: io::Error::new(io::ErrorKind::NotFound, "File not found")
    /// });
    /// ```
    pub fn emit_error(&mut self, error: &Error) {
        if let Some(ref mut cb) = self.error_cb {
            cb(error);
        }
    }

    /// Set the error callback
    ///
    /// ```
    /// use distinst::Installer;
    /// let mut installer = Installer::new();
    /// installer.on_error(|error| println!("{:?}", error));
    /// ```
    pub fn on_error<F: FnMut(&Error) + 'static>(&mut self, callback: F) {
        self.error_cb = Some(Box::new(callback));
    }

    /// Send a status message
    ///
    /// ```
    /// use distinst::{Installer, Status, Step};
    /// let mut installer = Installer::new();
    /// installer.emit_status(&Status {
    ///     step: Step::Extract,
    ///     percent: 50,
    /// });
    /// ```
    pub fn emit_status(&mut self, status: &Status) {
        if let Some(ref mut cb) = self.status_cb {
            cb(status);
        }
    }

    /// Set the status callback
    ///
    /// ```
    /// use distinst::Installer;
    /// let mut installer = Installer::new();
    /// installer.on_status(|status| println!("{:?}", status));
    /// ```
    pub fn on_status<F: FnMut(&Status) + 'static>(&mut self, callback: F) {
        self.status_cb = Some(Box::new(callback));
    }

    /// Install the system with the specified bootloader
    pub fn install(&mut self, config: &Config) {
        println!("Installing {:?}", config);

        let bootloader = Bootloader::detect();

        println!("Bootloader: {:?}", bootloader);

        for &step in [Step::Partition, Step::Format, Step::Extract, Step::Bootloader].iter() {
            for i in 0..11 {
                self.emit_status(&Status {
                    step: step,
                    percent: i * 10,
                });
                ::std::thread::sleep(::std::time::Duration::new(1, 0));
            }
        }
    }

    /// Get a list of disks, skipping loopback devices
    ///
    /// ```
    /// use distinst::Installer;
    /// let installer = Installer::new();
    /// let disks = installer.disks().unwrap();
    /// ```
    pub fn disks(&self) -> io::Result<Vec<Disk>> {
        let mut disks = Disk::all()?;
        disks.retain(|disk| {
            ! disk.name().starts_with("loop")
        });
        Ok(disks)
    }
}
