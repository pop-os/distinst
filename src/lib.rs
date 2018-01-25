//! A crate for installing Ubuntu distributions from a live squashfs

extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate libc;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate tempdir;

use self::partition::blockdev;
use std::{fs, io};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use tempdir::TempDir;

pub use chroot::Chroot;
pub use disk::{
    Bootloader, Disk, DiskError, Disks, FileSystemType, PartitionBuilder, PartitionFlag,
    PartitionInfo, PartitionTable, PartitionType, Sector,
};
pub use mount::{Mount, MountOption, Mounts};

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

const FSTAB_HEADER: &[u8] = b"# /etc/fstab: static file system information.
#
# Use 'blkid' to print the universally unique identifier for a
# device; this may be used with UUID= as a more robust way to name devices
# that works even if disks are added and removed. See fstab(5).
#
# <file system>  <mount point>  <type>  <options>  <dump>  <pass>
";

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
    pub lang:     String,
    pub remove:   String,
}

/// Installer error
#[derive(Debug)]
pub struct Error {
    pub step: Step,
    pub err:  io::Error,
}

/// Installer status
#[derive(Copy, Clone, Debug)]
pub struct Status {
    pub step:    Step,
    pub percent: i32,
}

/// An installer object
pub struct Installer {
    error_cb:  Option<Box<FnMut(&Error)>>,
    status_cb: Option<Box<FnMut(&Status)>>,
}

impl Installer {
    /// Create a new installer object
    ///
    /// ```
    /// use distinst::Installer;
    /// let installer = Installer::new();
    /// ```
    pub fn default() -> Installer {
        Installer {
            error_cb:  None,
            status_cb: None,
        }
    }

    /// Send an error message
    ///
    /// ```
    /// use distinst::{Error, Installer, Step};
    /// use std::io;
    /// let mut installer = Installer::new();
    /// installer.emit_error(&Error {
    ///     step: Step::Extract,
    ///     err:  io::Error::new(io::ErrorKind::NotFound, "File not found"),
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
    ///     step:    Step::Extract,
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
            Ok(squashfs) => if squashfs.exists() {
                info!("config.squashfs: found at {}", squashfs.display());
                squashfs
            } else {
                error!("config.squashfs: supplied file does not exist");
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "invalid squashfs path",
                ));
            },
            Err(err) => {
                error!("config.squashfs: {}", err);
                return Err(err);
            }
        };

        callback(20);

        for disk in &mut disks.0 {
            if let Err(why) = disk.unmount_all_partitions() {
                error!("unable to unmount partitions");
                return Err(io::Error::new(io::ErrorKind::Other, format!("{}", why)));
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

        Ok((squashfs, remove_pkgs))
    }

    fn partition<F: FnMut(i32)>(disks: &mut Disks, mut callback: F) -> io::Result<()> {
        for disk in &mut disks.0 {
            info!("{}: Committing changes to disk", disk.path().display());
            disk.commit()
                .map_err(|why| io::Error::new(io::ErrorKind::Other, format!("{}", why)))?;
            callback(100);
        }

        // This is to ensure that everything's been written and the OS is ready to proceed.
        ::std::thread::sleep(::std::time::Duration::from_secs(1));
        for disk in &disks.0 {
            blockdev(&disk.path(), &["--flushbufs", "--rereadpt"])?;
        }

        Ok(())
    }

    /// Mount all target paths defined within the provided `disks` configuration.
    fn mount(disks: &Disks, chroot: &str) -> io::Result<Mounts> {
        let targets = disks
            .as_ref()
            .iter()
            .flat_map(|disk| disk.partitions.iter())
            .filter(|part| !part.target.is_none() && !part.filesystem.is_none());

        let mut mounts = Vec::new();

        // The mount path will actually consist of the target concatenated with the root.
        // NOTE: It is assumed that the target is an absolute path.
        let paths: BTreeMap<PathBuf, (PathBuf, &'static str)> = targets
            .map(|target| {
                let target_path = target.target.as_ref().unwrap();
                let target_mount =
                    [chroot, target_path.to_string_lossy().to_string().as_str()].concat();

                let fs = match target.filesystem.unwrap() {
                    FileSystemType::Fat16 | FileSystemType::Fat32 => "vfat",
                    fs => fs.into(),
                };

                (
                    PathBuf::from(target_mount),
                    (target.device_path.clone(), fs),
                )
            })
            .collect();

        // Each mount directory will be created and then mounted before progressing to the next
        // mount in the map. The BTreeMap that the mount targets were collected into
        // will ensure that mounts are created and mounted in the correct order.
        for (target_mount, (device_path, filesystem)) in paths {
            if let Err(why) = fs::create_dir_all(&target_mount) {
                error!("unable to create '{}': {}", why, target_mount.display());
            }

            info!(
                "distinst: mounting {} to {}, with {}",
                device_path.display(),
                target_mount.display(),
                filesystem
            );

            mounts.push(Mount::new(
                &device_path,
                &target_mount,
                filesystem,
                0,
                None,
            )?);
        }

        Ok(Mounts(mounts))
    }

    fn extract<P: AsRef<Path>, F: FnMut(i32)>(
        squashfs: P,
        mount_dir: &'static str,
        callback: F,
    ) -> io::Result<()> {
        info!("distinst: Extracting {}", squashfs.as_ref().display());
        squashfs::extract(squashfs, mount_dir, callback)?;

        Ok(())
    }

    fn configure<P: AsRef<Path>, S: AsRef<str>, I: IntoIterator<Item = S>, F: FnMut(i32)>(
        disks: &Disks,
        mount_dir: P,
        bootloader: Bootloader,
        lang: &str,
        remove_pkgs: I,
        mut callback: F,
    ) -> io::Result<()> {
        let mount_dir = mount_dir.as_ref().canonicalize().unwrap();
        info!("distinst: Configuring on {}", mount_dir.display());
        let configure_dir = TempDir::new_in(mount_dir.join("tmp"), "distinst")?;
        let configure = configure_dir.path().join("configure.sh");

        {
            // Write our configuration file to `/tmp/configure.sh`.
            let mut file = fs::File::create(&configure)?;
            file.write_all(include_bytes!("configure.sh"))?;
            file.sync_all()?;
        }

        {
            // Write the /etc/fstab file using the target mounts defined in `disks`.
            let fstab = mount_dir.join("etc/fstab");
            let mut file = fs::File::create(&fstab)?;
            file.write_all(FSTAB_HEADER)?;
            file.write_all(disks.generate_fstab().as_bytes())?;
            file.sync_all()?;
        }

        {
            let mut chroot = Chroot::new(&mount_dir)?;
            let configure_chroot = configure.strip_prefix(&mount_dir).map_err(|err| {
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

            chroot.unmount(false)?;
        }

        configure_dir.close()?;

        callback(100);

        Ok(())
    }

    fn bootloader<F: FnMut(i32)>(
        disks: &Disks,
        mount_dir: &'static str,
        bootloader: Bootloader,
        mut callback: F,
    ) -> io::Result<()> {
        // Obtain the root device & partition, with an optional EFI device & partition.
        let ((root_dev, _), efi_opt) = disks.get_base_partitions(bootloader);

        let bootloader_dev = match efi_opt {
            Some((dev, _)) => dev,
            None => root_dev,
        };

        info!(
            "{}: installing bootloader for {:?}",
            bootloader_dev.display(),
            bootloader
        );

        {
            let boot_path = [mount_dir, "/boot"].concat();
            let efi_path = [&boot_path, "/efi"].concat();

            // Also ensure that the /boot/efi directory is created.
            if efi_opt.is_some() {
                fs::create_dir_all(&efi_path)?;
            }

            {
                let mut chroot = Chroot::new(mount_dir)?;

                {
                    let mut args = vec![];

                    args.push("--recheck".into());

                    match bootloader {
                        Bootloader::Bios => {
                            args.push("--target=i386-pc".into());
                        }
                        Bootloader::Efi => {
                            args.push("--target=x86_64-efi".into());
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
        }

        callback(100);

        Ok(())
    }

    /// Install the system with the specified bootloader
    pub fn install(&mut self, mut disks: Disks, config: &Config) -> io::Result<()> {
        let bootloader = Bootloader::detect();
        disks.verify_partitions(bootloader)?;

        info!("Installing {:?} with {:?}", config, bootloader);

        let mut status = Status {
            step:    Step::Init,
            percent: 0,
        };
        self.emit_status(&status);

        let (squashfs, remove_pkgs) = match Installer::initialize(&mut disks, config, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            Ok(value) => value,
            Err(err) => {
                error!("initialize: {}", err);
                let error = Error {
                    step: status.step,
                    err:  err,
                };
                self.emit_error(&error);
                return Err(error.err);
            }
        };

        status.step = Step::Partition;
        status.percent = 0;
        self.emit_status(&status);

        // Create, manipulate, and format partitions provided by the user.
        if let Err(err) = Installer::partition(&mut disks, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            error!("partition: {}", err);
            let error = Error {
                step: status.step,
                err:  err,
            };
            self.emit_error(&error);
            return Err(error.err);
        }

        status.step = Step::Extract;
        status.percent = 0;
        self.emit_status(&status);

        // Mount the temporary directory, and all of our mount targets.
        const CHROOT_ROOT: &str = "distinst";
        info!(
            "distinst: mounting temporary chroot directory at {}",
            CHROOT_ROOT
        );
        let mount_dir = TempDir::new(CHROOT_ROOT)?;
        let _mounts = Installer::mount(&disks, CHROOT_ROOT)?;

        // Extract the Linux image into the new chroot path
        if let Err(err) = Installer::extract(&squashfs, CHROOT_ROOT, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            error!("extract: {}", err);
            let error = Error {
                step: status.step,
                err:  err,
            };
            self.emit_error(&error);
            return Err(error.err);
        }

        status.step = Step::Configure;
        status.percent = 0;
        self.emit_status(&status);

        // Configure the new install, using the extracted image as a base.
        if let Err(err) = Installer::configure(
            &disks,
            CHROOT_ROOT,
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
                err:  err,
            };
            self.emit_error(&error);
            return Err(error.err);
        }

        status.step = Step::Bootloader;
        status.percent = 0;
        self.emit_status(&status);

        // Configure and install the bootloader
        if let Err(err) = Installer::bootloader(&disks, CHROOT_ROOT, bootloader, |percent| {
            status.percent = percent;
            self.emit_status(&status);
        }) {
            error!("bootloader: {}", err);
            let error = Error {
                step: status.step,
                err:  err,
            };
            self.emit_error(&error);
            return Err(error.err);
        }

        mount_dir.close()?;
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
