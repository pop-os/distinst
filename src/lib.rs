//! A crate for installing Ubuntu distributions from a live squashfs

extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate tempdir;

use tempdir::TempDir;

use std::{fs, io};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use partition::blockdev;
pub use disk::{Bootloader, Disk, DiskError, Disks, FileSystemType, PartitionBuilder, PartitionFlag,
               PartitionInfo, PartitionTable, PartitionType, Sector};
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
pub fn log<F: Fn(log::LogLevel, &str) + Send + Sync + 'static>(
    callback: F,
) -> Result<(), log::SetLoggerError> {
    match log::set_logger(|max_log_level| {
        max_log_level.set(log::LogLevelFilter::Debug);
        Box::new(logger::Logger::new(callback))
    }) {
        Ok(()) => {
            info!("Logging enabled");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// Installation step
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Step {
    Init,
    Partition,
    Extract,
    Configure,
    Bootloader,
}

/// Installer configuration
#[derive(Debug)]
pub struct Config {
    pub squashfs: String,
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

    fn initialize<F: FnMut(i32)>(
        disks: &mut Disks,
        config: &Config,
        mut callback: F,
    ) -> io::Result<(PathBuf, Vec<String>)> {
        info!("Initializing");

        let squashfs = match Path::new(&config.squashfs).canonicalize() {
            Ok(squashfs) => squashfs,
            Err(err) => {
                error!("config.squashfs: {}", err);
                return Err(err);
            }
        };

        callback(20);

        for partition in disks.0.iter_mut().flat_map(|p| p.partitions.iter_mut()) {
            if partition.is_swap() {
                info!("unswapping '{}'", partition.path().display(),);

                let status = Command::new("swapoff").arg(&partition.path()).status()?;
                if !status.success() {
                    error!("config.disk: failed to swapoff with status {}", status);
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("swapoff failed with status: {}", status),
                    ));
                }
            } else if let Some(ref mount) = partition.mount_point {
                info!(
                    "unmounting {}, which is mounted at {}",
                    partition.path().display(),
                    mount.display()
                );

                let status = Command::new("umount").arg(&partition.path()).status()?;
                if !status.success() {
                    error!("config.disk: failed to umount with status {}", status);
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("umount failed with status: {}", status),
                    ));
                }
            }

            partition.mount_point = None;
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

        Ok((squashfs, remove_pkgs))
    }

    fn partition<F: FnMut(i32)>(disks: &mut Disks, mut callback: F) -> io::Result<()> {
        for disk in disks.0.iter_mut() {
            info!("{}: Committing changes to disk", disk.path().display());
            disk.commit()
                .map_err(|why| io::Error::new(io::ErrorKind::Other, format!("{}", why)))?;

            info!("{}: Rereading partition table", disk.path().display());
            blockdev(&disk.path(), &["--flushbufs", "--rereadpt"])?;
            callback(100);
        }

        Ok(())
    }

    fn extract<P: AsRef<Path>, F: FnMut(i32)>(
        squashfs: P,
        disks: &mut Disks,
        callback: F,
    ) -> io::Result<()> {
        let (_root_dev, root) = disks.find_partition(Path::new("/"))
            .expect("verify_partitions() should have ensured that a root partition was created");

        info!(
            "{}: Extracting {}",
            root.path().display(),
            squashfs.as_ref().display()
        );

        let mount_dir = TempDir::new("distinst")?;

        {
            let part_dev = root.path();
            let mut mount = Mount::new(&part_dev, mount_dir.path(), &[])?;

            {
                squashfs::extract(squashfs, mount_dir.path(), callback)?;
            }

            mount.unmount(false)?;
        }
        mount_dir.close()?;

        Ok(())
    }

    fn configure<S: AsRef<str>, I: IntoIterator<Item = S>, F: FnMut(i32)>(
        disks: &mut Disks,
        bootloader: Bootloader,
        lang: &str,
        remove_pkgs: I,
        mut callback: F,
    ) -> io::Result<()> {
        let ((_root_dev, root_part), efi_opt) = disks.get_base_partitions(bootloader);
        let mount_dir = TempDir::new("distinst")?;

        {
            let root_dev = root_part.path();
            let mut mount = Mount::new(&root_dev, mount_dir.path(), &[])?;

            let mut efi_mount_opt = match efi_opt {
                Some((_, efi)) => {
                    let efi_path = mount_dir.path().join("boot").join("efi");
                    fs::create_dir_all(&efi_path)?;
                    let efi_dev = efi.path();
                    Some(Mount::new(&efi_dev, &efi_path, &[])?)
                }
                None => None,
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
                        let configure_chroot =
                            configure.strip_prefix(mount_dir.path()).map_err(|err| {
                                io::Error::new(
                                    io::ErrorKind::Other,
                                    format!("Path::strip_prefix failed: {}", err),
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

                        if !status.success() {
                            return Err(io::Error::new(
                                io::ErrorKind::Other,
                                format!("configure.sh failed with status: {}", status),
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

    fn bootloader<F: FnMut(i32)>(
        disks: &mut Disks,
        bootloader: Bootloader,
        mut callback: F,
    ) -> io::Result<()> {
        // Obtain the root device & partition, with an optional EFI device & partition.
        let ((root_dev, root_part), efi_opt) = disks.get_base_partitions(bootloader);

        let bootloader_dev = match efi_opt {
            Some((dev, _)) => dev,
            None => root_dev,
        };

        info!(
            "{}: installing bootloader for {:?}",
            bootloader_dev.display(),
            bootloader
        );

        let mount_dir = TempDir::new("distinst")?;

        {
            let boot_path = mount_dir.path().join("boot");
            let efi_path = boot_path.join("efi");

            let root_part = root_part.path();
            let mut mount = Mount::new(&root_part, mount_dir.path(), &[])?;

            let mut efi_mount_opt = match efi_opt {
                Some((_, efi)) => {
                    fs::create_dir_all(&efi_path)?;
                    let efi_dev = efi.path();
                    Some(Mount::new(&efi_dev, &efi_path, &[])?)
                }
                None => None,
            };

            {
                let mut chroot = Chroot::new(mount_dir.path())?;

                {
                    let mut args = vec![];

                    args.push(format!("--recheck"));

                    match bootloader {
                        Bootloader::Bios => {
                            args.push(format!("--target=i386-pc"));
                        }
                        Bootloader::Efi => {
                            args.push(format!("--target=x86_64-efi"));
                        }
                    }

                    args.push(bootloader_dev.to_str().unwrap().to_owned());

                    let status = chroot.command("grub-install", args.iter())?;
                    if !status.success() {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            format!("grub-install failed with status: {}", status),
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
    pub fn install(&mut self, mut disks: Disks, config: &Config) -> io::Result<()> {
        let bootloader = Bootloader::detect();
        disks.verify_partitions(bootloader)?;

        info!("Installing {:?} with {:?}", config, bootloader);

        let mut status = Status {
            step: Step::Init,
            percent: 0,
        };
        self.emit_status(&status);

        let (squashfs, remove_pkgs) = match Installer::initialize(&mut disks, &config, |percent| {
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

        status.step = Step::Partition;
        status.percent = 0;
        self.emit_status(&status);

        if let Err(err) = Installer::partition(&mut disks, |percent| {
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

        status.step = Step::Extract;
        status.percent = 0;
        self.emit_status(&status);

        if let Err(err) = Installer::extract(&squashfs, &mut disks, |percent| {
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

        if let Err(err) = Installer::configure(
            &mut disks,
            bootloader,
            &config.lang,
            &remove_pkgs,
            |percent| {
                status.percent = percent;
                self.emit_status(&status);
            },
        ) {
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

        if let Err(err) = Installer::bootloader(&mut disks, bootloader, |percent| {
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
