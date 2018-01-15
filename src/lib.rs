//! A crate for installing Ubuntu distributions from a live squashfs

extern crate failure;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate log;
extern crate tempdir;
extern crate libparted;

use tempdir::TempDir;

use std::{fs, io};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use disk::{Disk, Disks, PartitionBuilder, Sector, mklabel};
pub use libparted::PartitionFlag;
pub use disk::{FileSystemType, PartitionTable, PartitionType};
use format::mkfs;
use partition::{blockdev, parted};
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
    pub lang: String,
    pub remove: String,
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

    fn initialize<F: FnMut(i32)>(config: &Config, mut callback: F) -> io::Result<(PathBuf, Disk, Vec<String>)> {
        info!("Initializing");

        let squashfs = match Path::new(&config.squashfs).canonicalize() {
            Ok(squashfs) => squashfs,
            Err(err) => {
                error!("config.squashfs: {}", err);
                return Err(err);
            }
        };

        callback(20);

        let disk = match Disk::from_name(&config.disk) {
            Ok(disk) => disk,
            Err(why) => {
                error!("config.disk: {}", why);
                return Err(io::Error::new(io::ErrorKind::Other, why.to_string()));
            }
        };

        callback(40);

        for partition in &disk.partitions {
            if partition.is_swap() {
                info!("Unswapping '{}': {} is swapped",
                    disk.path().display(), partition.path().display(),
                );

                let status = Command::new("swapoff").arg(&partition.path()).status()?;
                if ! status.success() {
                    error!("config.disk: failed to swapoff with status {}", status);
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("swapoff failed with status: {}", status)
                    ));
                }
            } else if let Some(ref mount) = partition.mount_point {
                info!(
                    "Unmounting '{}': {} is mounted at {}",
                    disk.path().display(), partition.path().display(), mount.display()
                );

                let status = Command::new("umount").arg(&partition.path()).status()?;
                if ! status.success() {
                    error!("config.disk: failed to umount with status {}", status);
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("umount failed with status: {}", status)
                    ));
                }
            }
        }

        callback(80);

        let mut remove_pkgs = Vec::new();
        {
            let file = match fs::File::open(&config.remove) {
                Ok(file) => file,
                Err(err) => {
                    error!("config.remove: {}", err);
                    return Err(err);
                }
            };

            for line_res in io::BufReader::new(file).lines() {
                match line_res {
                    Ok(line) => remove_pkgs.push(line),
                    Err(err) => {
                        error!("config.remove: {}", err);
                        return Err(err);
                    }
                }
            }
        }

        Ok((squashfs, disk, remove_pkgs))
    }

    fn partition<F: FnMut(i32)>(disk: &mut Disk, bootloader: Bootloader, mut callback: F) -> io::Result<()> {
        info!("{}: Partitioning for {:?}", disk.path().display(), bootloader);

        match bootloader {
            Bootloader::Bios => {
                mklabel(&disk.path(), PartitionTable::Msdos)?;
                callback(33);

                let start = disk.get_sector(Sector::Start);
                let end = disk.get_sector(Sector::End);
                disk.add_partition(
                    PartitionBuilder::new(start, end, FileSystemType::Ext4)
                        .partition_type(PartitionType::Primary)
                        .flag(PartitionFlag::PED_PARTITION_BOOT)
                )?;

                callback(66);
            },
            Bootloader::Efi => {
                mklabel(&disk.path(), PartitionTable::Gpt)?;
                callback(25);

                let mut start = disk.get_sector(Sector::Start);
                let mut end = disk.get_sector(Sector::Megabyte(512));
                disk.add_partition(
                    PartitionBuilder::new(start, end, FileSystemType::Fat32)
                        .partition_type(PartitionType::Primary)
                        .flag(PartitionFlag::PED_PARTITION_ESP)
                )?;

                callback(50);

                start = disk.get_sector(Sector::Megabyte(512));
                end = disk.get_sector(Sector::End);
                disk.add_partition(
                    PartitionBuilder::new(start, end, FileSystemType::Ext4)
                        .partition_type(PartitionType::Primary)
                )?;

                callback(75);
            }
        }

        info!("{}: Committing changes to disk", disk.path().display());
        disk.commit().map_err(|why| {
            io::Error::new(io::ErrorKind::Other, format!("{}", why))
        })?;

        info!("{}: Rereading partition table", disk.path().display());
        blockdev(&disk.path(), &["--rereadpt"])?;
        callback(100);

        Ok(())
    }

    fn format<F: FnMut(i32)>(disk: &mut Disk, bootloader: Bootloader, mut callback: F) -> io::Result<()> {
        let disk_dev = disk.path();
        info!("{}: Formatting for {:?}", disk_dev.display(), bootloader);

        match bootloader {
            Bootloader::Bios => {
                let part = disk.partitions.iter().next().ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                let part_dev = part.path();
                info!("{}: Formatting ext4 root partition", part_dev.display());
                mkfs(&part_dev, FileSystemType::Ext4)?;
            },
            Bootloader::Efi => {
                {
                    let part = disk.partitions.iter().next().ok_or(
                        io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                    )?;

                    let part_dev = part.path();
                    info!("{}: Formatting fat32 efi partition", part_dev.display());
                    mkfs(&part_dev, FileSystemType::Fat32)?;
                }

                callback(50);

                {
                    let part = disk.partitions.iter().skip(1).next().ok_or(
                        io::Error::new(io::ErrorKind::NotFound, "Partition 1 not found")
                    )?;

                    let part_dev = part.path();
                    info!("{}: Formatting ext4 root partition", part_dev.display());
                    mkfs(&part_dev, FileSystemType::Ext4)?;
                }
            }
        }

        callback(100);

        Ok(())
    }

    fn extract<P: AsRef<Path>, F: FnMut(i32)>(squashfs: P, disk: &mut Disk, bootloader: Bootloader, callback: F) -> io::Result<()> {
        let disk_dev = disk.path();
        info!("{}: Extracting {}", disk_dev.display(), squashfs.as_ref().display());

        let part = match bootloader {
            Bootloader::Bios => {
                disk.partitions.iter().next().ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?
            },
            Bootloader::Efi => {
                disk.partitions.iter().skip(1).next().ok_or(
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

    fn configure<S: AsRef<str>, I: IntoIterator<Item=S>, F: FnMut(i32)>(disk: &mut Disk, bootloader: Bootloader, lang: &str, remove_pkgs: I, mut callback: F) -> io::Result<()> {
        let disk_dev = disk.path();
        info!("{}: Configuring for {:?}", disk_dev.display(), bootloader);

        let (part, efi_opt) = match bootloader {
            Bootloader::Bios => {
                let part = disk.partitions.iter().next().ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                (part, None)
            },
            Bootloader::Efi => {
                let efi = disk.partitions.iter().next().ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                let part = disk.partitions.iter().skip(1).next().ok_or(
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

                        let mut args = vec![
                            // Clear existing environment
                            "-i".to_string(),
                            // Set language to config setting
                            format!("LANG={}", lang),
                            // Run configure script with bash
                            "bash".to_string(),
                            // Path to configure script in chroot
                            configure_chroot.to_str().unwrap().to_string(),
                            // Install appropriate grub package
                            grub_pkg.to_string(),
                        ];

                        for pkg in remove_pkgs {
                            // Remove installer packages
                            args.push(format!("-{}", pkg.as_ref()));
                        }

                        let status = chroot.command("/usr/bin/env", args.iter())?;

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

        let (part, efi_opt) = match bootloader {
            Bootloader::Bios => {
                let part = disk.partitions.iter().next().ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                (part, None)
            },
            Bootloader::Efi => {
                let efi = disk.partitions.iter().next().ok_or(
                    io::Error::new(io::ErrorKind::NotFound, "Partition 0 not found")
                )?;

                let part = disk.partitions.iter().skip(1).next().ok_or(
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

                    args.push(disk_dev.to_str().unwrap().to_owned());

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

        let (squashfs, mut disk, remove_pkgs) = match Installer::initialize(&config, |percent| {
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

        if let Err(err) = Installer::configure(&mut disk, bootloader, &config.lang, &remove_pkgs, |percent| {
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
    pub fn disks(&self) -> io::Result<Disks> {
        Disks::probe_devices()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{}", err)))
    }
}
