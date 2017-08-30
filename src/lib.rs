//! A crate for installing Ubuntu distributions from a live squashfs

use std::io::Result;

use disk::Disk;

#[doc(hidden)]
pub use c::*;

mod c;
mod disk;

/// An installer object
pub struct Installer {
    status_cb: Option<Box<FnMut(u32)>>
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
            status_cb: None,
        }
    }

    /// Send a status message
    ///
    /// ```
    /// use distinst::Installer;
    /// let mut installer = Installer::new();
    /// installer.emit_status(0);
    /// ```
    pub fn emit_status(&mut self, status: u32) {
        if let Some(ref mut status_cb) = self.status_cb {
            status_cb(status);
        }
    }

    /// Set the status callback
    ///
    /// ```
    /// use distinst::Installer;
    /// let mut installer = Installer::new();
    /// installer.on_status(|status| println!("{}", status));
    /// ```
    pub fn on_status<F: FnMut(u32) + 'static>(&mut self, callback: F) {
        self.status_cb = Some(Box::new(callback));
    }

    /// Get a list of disks, skipping loopback devices
    ///
    /// ```
    /// use distinst::Installer;
    /// let installer = Installer::new();
    /// let disks = installer.disks().unwrap();
    /// ```
    pub fn disks(&self) -> Result<Vec<Disk>> {
        let mut disks = Disk::all()?;
        disks.retain(|disk| {
            ! disk.name().starts_with("loop")
        });
        Ok(disks)
    }
}
