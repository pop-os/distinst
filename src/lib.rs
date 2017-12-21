//! A crate for installing Ubuntu distributions from a live squashfs

#[macro_use]
extern crate log;
extern crate tempdir;

use tempdir::TempDir;

use std::{fs, io};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use disk::Disk;
use format::{MkfsKind, mkfs};
use partition::{parted, partprobe, sync};
pub use chroot::Chroot;
pub use mount::{Mount, MountOption};

#[doc(hidden)]
pub use c::*;

mod c;
mod chroot;
mod disk;
mod format;
mod logger;
mod mount;
mod partition;
mod squashfs;

/// Initialize logging
pub fn log<F: Fn(log::LogLevel, &str) + Send + Sync + 'static>(callback: F) -> Result<(), log::SetLoggerError> {
    match log::set_logger(|max_log_level| {
        max_log_level.set(log::LogLevelFilter::Debug);
        Box::new(logger::Logger::new(callback))
    }) {
        Ok(()) => {
            info!("Logging enabled");
            Ok(())
        },
        Err(err) => Err(err)
    }
}

/// Bootloader type
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Bootloader {
    Bios,
    Efi,
}

impl Bootloader {
    pub fn detect() -> Bootloader {
        if Path::new("/sys/firmware/efi").is_dir() {
            Bootloader::Efi
        } else {
            Bootloader::Bios
        }
    }
}

/// Installation step
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Step {
    Init,
    Partition,
    Format,
    Extract,
    Configure,
    Bootloader,
}

/// Installer configuration
#[derive(Debug)]
pub struct Config {
    pub squashfs: String,
    pub disk: String,
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

    fn initialize<F: FnMut(i32)>(config: &Config, mut callback: F) -> io::Result<(PathBuf, Disk)> {
        info!("Initializing");

        let squashfs = match Path::new(&config.squashfs).canonicalize() {
            Ok(squashfs) => squashfs,
            Err(err) => {
                error!("config.squashfs: {}", err);
                return Err(err);
            }
        };

        callback(25);

        let disk = match Disk::from_name(&config.disk) {
            Ok(disk) => disk,
            Err(err) => {
                error!("config.disk: {}", err);
                return Err(err);
            }
        };

        callback(50);

        for mount in disk.mounts()? {
            info!(
                "Unmounting '{}': {:?} is mounted at {:?}",
                disk.name(), mount.source, mount.dest
            );

            let status = Command::new("umount").arg(&mount.source).status()?;
            if ! status.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("umount failed with status: {}", status)
                ));
            }
        }

        callback(75);

        for swap in disk.swaps()? {
            info!(
                "Unswapping '{}': {:?} is swapped",
                disk.name(), swap.source,
            );

            let status = Command::new("swapoff").arg(&swap.source).status()?;
            if ! status.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("swapoff failed with status: {}", status)
                ));
            }
        }

        callback(100);

        Ok((squashfs, disk))
    }

    fn partition<F: FnMut(i32)>(disk: &mut Disk, bootloader: Bootloader, mut callback: F) -> io::Result<()> {
        let disk_dev = disk.path();
        info!("{}: Partitioning for {:?}", disk_dev.display(), bootloader);

        // TODO: Use libparted
        match bootloader {
            Bootloader::Bios => {
                parted(&disk_dev, &["mklabel", "msdos"])?;
                callback(33);

                parted(&disk_dev, &["mkpart", "primary", "ext4", "0%", "100%"])?;
                parted(&disk_dev, &["set", "1", "boot", "on"])?;
                callback(66);
            },
            Bootloader::Efi => {
                parted(&disk_dev, &["mklabel", "gpt"])?;
                callback(25);

                parted(&disk_dev, &["mkpart", "primary", "fat32", "0%", "512M"])?;
                parted(&disk_dev, &["set", "1", "esp", "on"])?;
                callback(50);

                parted(&disk_dev, &["mkpart", "primary", "ext4", "512M", "100%"])?;
                callback(75);
            }
        }

        info!("{}: Rereading partition table", disk_dev.display());
        sync()?;
        partprobe(&disk_dev)?;
        sync()?;
        callback(100);

        Ok(())
    }

    fn format<F: FnMut(i32)>(disk: &mut Disk, bootloader: Bootloader, mut callback: F) -> io::Result<()> {
        let disk_dev = disk.path();
        info!("{}: Formatting for {:?}", disk_dev.display(), bootloader);

        // TODO: Use libparted
        let parts = disk.parts()?;
        match bootloader {
            Bootloader::Bios => {
                let part = parts.get(0).ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                let part_dev = part.path();
                info!("{}: Formatting ext4 root partition", part_dev.display());
                mkfs(&part_dev, MkfsKind::Ext4)?;
            },
            Bootloader::Efi => {
                {
                    let part = parts.get(1).ok_or(
                        io::Error::new(io::ErrorKind::NotFound, "Partition 1 not found")
                    )?;

                    let part_dev = part.path();
                    info!("{}: Formatting ext4 root partition", part_dev.display());
                    mkfs(&part_dev, MkfsKind::Ext4)?;
                }

                callback(50);

                {
                    let part = parts.get(0).ok_or(
                        io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                    )?;

                    let part_dev = part.path();
                    info!("{}: Formatting fat32 efi partition", part_dev.display());
                    mkfs(&part_dev, MkfsKind::Fat32)?;
                }
            }
        }

        callback(100);

        Ok(())
    }

    fn extract<P: AsRef<Path>, F: FnMut(i32)>(squashfs: P, disk: &mut Disk, bootloader: Bootloader, callback: F) -> io::Result<()> {
        let disk_dev = disk.path();
        info!("{}: Extracting {}", disk_dev.display(), squashfs.as_ref().display());

        let parts = disk.parts()?;
        let part = match bootloader {
            Bootloader::Bios => {
                parts.get(0).ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?
            },
            Bootloader::Efi => {
                parts.get(1).ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 1 not found")
                )?
            }
        };

        let mount_dir = TempDir::new("distinst")?;

        {
            let part_dev = part.path();
            let mut mount = Mount::new(&part_dev, mount_dir.path(), &[])?;

            {
                squashfs::extract(squashfs, mount_dir.path(), callback)?;
            }

            mount.unmount(false)?;

        }
        mount_dir.close()?;

        Ok(())
    }

    fn configure<F: FnMut(i32)>(disk: &mut Disk, bootloader: Bootloader, mut callback: F) -> io::Result<()> {
        let disk_dev = disk.path();
        info!("{}: Configuring for {:?}", disk_dev.display(), bootloader);

        let parts = disk.parts()?;
        let (part, efi_opt) = match bootloader {
            Bootloader::Bios => {
                let part = parts.get(0).ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                (part, None)
            },
            Bootloader::Efi => {
                let efi = parts.get(0).ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                let part = parts.get(1).ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 1 not found")
                )?;

                (part, Some(efi))
            }
        };

        let mount_dir = TempDir::new("distinst")?;

        {
            let part_dev = part.path();
            let mut mount = Mount::new(&part_dev, mount_dir.path(), &[])?;

            let mut efi_mount_opt = match efi_opt {
                Some(efi) => {
                    let efi_path = mount_dir.path().join("boot").join("efi");
                    fs::create_dir_all(&efi_path)?;
                    let efi_dev = efi.path();
                    Some(Mount::new(&efi_dev, &efi_path, &[])?)
                },
                None => None
            };

            {
                let configure_dir = TempDir::new_in(mount_dir.path().join("tmp"), "distinst")?;

                {
                    let configure = configure_dir.path().join("configure.sh");

                    {
                        let mut file = fs::File::create(&configure)?;
                        file.write_all(include_bytes!("configure.sh"))?;
                        file.sync_all()?;
                    }

                    let mut chroot = Chroot::new(mount_dir.path())?;

                    {
                        let configure_chroot = configure.strip_prefix(mount_dir.path()).map_err(|err| {
                            io::Error::new(
                                io::ErrorKind::Other,
                                format!("Path::strip_prefix failed: {}", err)
                            )
                        })?;

                        let grub_pkg = match bootloader {
                            Bootloader::Bios => "grub-pc",
                            Bootloader::Efi => "grub-efi-amd64-signed",
                        };

                        let status = chroot.command("/bin/bash", [
                            configure_chroot.to_str().unwrap(),
                            grub_pkg
                        ].iter())?;

                        if ! status.success() {
                            return Err(io::Error::new(
                                io::ErrorKind::Other,
                                format!("configure.sh failed with status: {}", status)
                            ));
                        }
                    }

                    chroot.unmount(false)?;
                }

                configure_dir.close()?;
            }

            if let Some(mut efi_mount) = efi_mount_opt.take() {
                efi_mount.unmount(false)?;
            }

            mount.unmount(false)?;
        }

        mount_dir.close()?;

        callback(100);

        Ok(())
    }

    fn bootloader<F: FnMut(i32)>(disk: &mut Disk, bootloader: Bootloader, mut callback: F) -> io::Result<()> {
        let disk_dev = disk.path();
        info!("{}: Installing bootloader for {:?}", disk_dev.display(), bootloader);

        let parts = disk.parts()?;
        let (part, efi_opt) = match bootloader {
            Bootloader::Bios => {
                let part = parts.get(0).ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                (part, None)
            },
            Bootloader::Efi => {
                let efi = parts.get(0).ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                let part = parts.get(1).ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 1 not found")
                )?;

                (part, Some(efi))
            }
        };

        let mount_dir = TempDir::new("distinst")?;

        {
            let boot_path = mount_dir.path().join("boot");
            let efi_path = boot_path.join("efi");

            let part_dev = part.path();
            let mut mount = Mount::new(&part_dev, mount_dir.path(), &[])?;

            let mut efi_mount_opt = match efi_opt {
                Some(efi) => {
                    fs::create_dir_all(&efi_path)?;
                    let efi_dev = efi.path();
                    Some(Mount::new(&efi_dev, &efi_path, &[])?)
                },
                None => None
            };

            {
                let mut chroot = Chroot::new(mount_dir.path())?;

                {
                    let mut args = vec![];

                    args.push(format!("--recheck"));

                    match bootloader {
                        Bootloader::Bios => {
                            args.push(format!("--target=i386-pc"));
                        },
                        Bootloader::Efi => {
                            args.push(format!("--target=x86_64-efi"));
                        }
                    }

                    args.push(disk_dev.into_os_string().into_string().unwrap());

                    let status = chroot.command("grub-install", args.iter())?;
                    if ! status.success() {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!("grub-install failed with status: {}", status)
                        ));
                    }
                }

                chroot.unmount(false)?;
            }

            if let Some(mut efi_mount) = efi_mount_opt.take() {
                efi_mount.unmount(false)?;
            }

            mount.unmount(false)?;
        }

        mount_dir.close()?;

        callback(100);

        Ok(())
    }

    /// Install the system with the specified bootloader
    pub fn install(&mut self, config: &Config) -> io::Result<()> {
        info!("Installing {:?}", config);

        let mut status = Status {
            step: Step::Init,
            percent: 0,
        };
        self.emit_status(&status);

        let (squashfs, mut disk) = match Installer::initialize(&config, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            Ok(value) => value,
            Err(err) => {
                error!("initialize: {}", err);
                let error = Error {
                    step: status.step,
                    err: err,
                };
                self.emit_error(&error);
                return Err(error.err);
            }
        };

        let bootloader = Bootloader::detect();

        info!("Detected {:?}", bootloader);

        status.step = Step::Partition;
        status.percent = 0;
        self.emit_status(&status);

        if let Err(err) = Installer::partition(&mut disk, bootloader, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            error!("partition: {}", err);
            let error = Error {
                step: status.step,
                err: err,
            };
            self.emit_error(&error);
            return Err(error.err);
        }

        status.step = Step::Format;
        status.percent = 0;
        self.emit_status(&status);

        if let Err(err) = Installer::format(&mut disk, bootloader, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            error!("format: {}", err);
            let error = Error {
                step: status.step,
                err: err,
            };
            self.emit_error(&error);
            return Err(error.err);
        }

        status.step = Step::Extract;
        status.percent = 0;
        self.emit_status(&status);

        if let Err(err) = Installer::extract(&squashfs, &mut disk, bootloader, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            error!("extract: {}", err);
            let error = Error {
                step: status.step,
                err: err,
            };
            self.emit_error(&error);
            return Err(error.err);
        }

        status.step = Step::Configure;
        status.percent = 0;
        self.emit_status(&status);

        if let Err(err) = Installer::configure(&mut disk, bootloader, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            error!("configure: {}", err);
            let error = Error {
                step: status.step,
                err: err,
            };
            self.emit_error(&error);
            return Err(error.err);
        }

        status.step = Step::Bootloader;
        status.percent = 0;
        self.emit_status(&status);

        if let Err(err) = Installer::bootloader(&mut disk, bootloader, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            error!("bootloader: {}", err);
            let error = Error {
                step: status.step,
                err: err,
            };
            self.emit_error(&error);
            return Err(error.err);
        }

        Ok(())
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
