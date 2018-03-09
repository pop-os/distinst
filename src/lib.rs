//! A crate for installing Ubuntu distributions from a live squashfs

#![allow(unknown_lints)]

#[macro_use]
extern crate derive_new;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate itertools;
extern crate libc;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate tempdir;

use disk::external::{blockdev, pvs, vgactivate, vgdeactivate};
use itertools::Itertools;
use std::{fs, io};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{File, Permissions};
use std::io::{BufRead, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering, ATOMIC_BOOL_INIT};
use tempdir::TempDir;

pub use automatic::{InstallOption, InstallOptions};
pub use chroot::Chroot;
pub use disk::{
    Bootloader, Disk, DiskError, DiskExt, Disks, FileSystemType, LvmDevice, LvmEncryption,
    PartitionBuilder, PartitionFlag, PartitionInfo, PartitionTable, PartitionType, Sector,
};
pub use disk::mount::{Mount, Mounts};

mod automatic;
mod chroot;
mod disk;
mod hostname;
mod logger;
mod squashfs;

/// When set to true, this will stop the installation process.
pub static KILL_SWITCH: AtomicBool = ATOMIC_BOOL_INIT;

/// Exits before the unsquashfs step
pub static PARTITIONING_TEST: AtomicBool = ATOMIC_BOOL_INIT;

/// Self-explanatory -- the fstab file will be generated with this header.
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
    /// Hostname to assign to the installed system.
    pub hostname: String,
    /// Keyboard configuration to set for the installed system.
    pub keyboard: String,
    /// The locale to use for the installed system.
    pub lang: String,
    /// The file that contains a list of packages to remove.
    pub remove: String,
    /// The archive (`tar` or `squashfs`) which contains the base system.
    pub squashfs: String,
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
    /// ```ignore,rust
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
    /// ```ignore,rust
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
    /// ```ignore,rust
    /// use distinst::Installer;
    /// let mut installer = Installer::new();
    /// installer.on_error(|error| println!("{:?}", error));
    /// ```
    pub fn on_error<F: FnMut(&Error) + 'static>(&mut self, callback: F) {
        self.error_cb = Some(Box::new(callback));
    }

    /// Send a status message
    ///
    /// ```ignore,rust
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
    /// ```ignore,rust
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

        // Deactivate any open logical volumes & close any encrypted partitions.
        if let Err(why) = disks.deactivate_device_maps() {
            error!("lvm deactivation error: {}", why);
            return Err(io::Error::new(io::ErrorKind::Other, format!("{}", why)));
        }

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

        disks.verify_keyfile_paths()?;

        callback(20);

        for disk in disks.get_physical_devices_mut() {
            if let Err(why) = disk.unmount_all_partitions() {
                error!("unable to unmount partitions");
                return Err(io::Error::new(io::ErrorKind::Other, format!("{}", why)));
            }
        }

        callback(80);

        let mut remove_pkgs = Vec::new();
        {
            let file = match File::open(&config.remove) {
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

        callback(100);

        Ok((squashfs, remove_pkgs))
    }

    /// Apply all partitioning and formatting changes to the disks
    /// configuration specified.
    fn partition<F: FnMut(i32)>(disks: &mut Disks, mut callback: F) -> io::Result<()> {
        for disk in disks.get_physical_devices_mut() {
            info!(
                "libdistinst: {}: Committing changes to disk",
                disk.path().display()
            );
            disk.commit()
                .map_err(|why| io::Error::new(io::ErrorKind::Other, format!("{}", why)))?;
            callback(100);
        }

        let pvs = pvs().map_err(|why| io::Error::new(io::ErrorKind::Other, format!("{}", why)))?;

        // TODO: Make this a method?

        // Obtain a list of volume groups in the configuration.
        let vgs = disks
            .get_physical_devices()
            .iter()
            .flat_map(|disk| {
                disk.get_partitions()
                    .iter()
                    .filter_map(|part| match pvs.get(&part.device_path) {
                        Some(&Some(ref vg)) => Some(vg.clone()),
                        _ => None,
                    })
            })
            .unique()
            .collect::<Vec<String>>();

        // Deactivate logical volumes so that blockdev will not fail.
        for vg in &vgs {
            vgdeactivate(vg);
        }

        // This is to ensure that everything's been written and the OS is ready to
        // proceed.
        ::std::thread::sleep(::std::time::Duration::from_secs(1));
        for disk in disks.get_physical_devices() {
            blockdev(&disk.path(), &["--flushbufs", "--rereadpt"])?;
        }
        ::std::thread::sleep(::std::time::Duration::from_secs(1));

        // Reactivate the logical volumes.
        for vg in vgs {
            vgactivate(&vg);
        }

        disks
            .commit_logical_partitions()
            .map_err(|why| io::Error::new(io::ErrorKind::Other, format!("{}", why)))
    }

    /// Mount all target paths defined within the provided `disks`
    /// configuration.
    fn mount(disks: &Disks, chroot: &Path) -> io::Result<Mounts> {
        let physical_targets = disks
            .get_physical_devices()
            .iter()
            .flat_map(|disk| disk.partitions.iter())
            .filter(|part| !part.target.is_none() && !part.filesystem.is_none());

        let logical_targets = disks
            .get_logical_devices()
            .iter()
            .flat_map(|disk| disk.partitions.iter())
            .filter(|part| !part.target.is_none() && !part.filesystem.is_none());

        let targets = physical_targets.chain(logical_targets);

        let mut mounts = Vec::new();

        // The mount path will actually consist of the target concatenated with the
        // root. NOTE: It is assumed that the target is an absolute path.
        let paths: BTreeMap<PathBuf, (PathBuf, &'static str)> = targets
            .map(|target| {
                // Path mangling commences here, since we need to concatenate an absolute
                // path onto another absolute path, and the standard library opts for
                // overwriting the original path when doing that.
                let target_mount: PathBuf = {
                    // Ensure that the chroot path has the ending '/'.
                    let chroot = chroot.as_os_str().as_bytes();
                    let mut target_mount: Vec<u8> = if chroot[chroot.len() - 1] == b'/' {
                        chroot.to_owned()
                    } else {
                        let mut temp = chroot.to_owned();
                        temp.push(b'/');
                        temp
                    };

                    // Cut the ending '/' from the target path if it exists.
                    let target_path = target.target.as_ref().unwrap().as_os_str().as_bytes();
                    let target_path = if target_path[0] == b'/' {
                        if target_path.len() > 1 {
                            &target_path[1..]
                        } else {
                            b""
                        }
                    } else {
                        target_path
                    };

                    // Append the target path to the chroot, and return it as a path type.
                    target_mount.extend_from_slice(target_path);
                    PathBuf::from(OsString::from_vec(target_mount))
                };

                let fs = match target.filesystem.clone().unwrap() {
                    FileSystemType::Fat16 | FileSystemType::Fat32 => "vfat",
                    fs => fs.into(),
                };

                (target_mount, (target.device_path.clone(), fs))
            })
            .collect();

        // Each mount directory will be created and then mounted before progressing to
        // the next mount in the map. The BTreeMap that the mount targets were
        // collected into will ensure that mounts are created and mounted in
        // the correct order.
        for (target_mount, (device_path, filesystem)) in paths {
            if let Err(why) = fs::create_dir_all(&target_mount) {
                error!("unable to create '{}': {}", why, target_mount.display());
            }

            info!(
                "libdistinst: mounting {} to {}, with {}",
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

    /// Extracts the squashfs image into the new install
    fn extract<P: AsRef<Path>, F: FnMut(i32)>(
        squashfs: P,
        mount_dir: P,
        callback: F,
    ) -> io::Result<()> {
        info!("libdistinst: Extracting {}", squashfs.as_ref().display());
        squashfs::extract(squashfs, mount_dir, callback)?;

        Ok(())
    }

    /// Configures the new install after it has been extracted.
    fn configure<P: AsRef<Path>, S: AsRef<str>, I: IntoIterator<Item = S>, F: FnMut(i32)>(
        disks: &Disks,
        mount_dir: P,
        bootloader: Bootloader,
        config: &Config,
        remove_pkgs: I,
        mut callback: F,
    ) -> io::Result<()> {
        let mount_dir = mount_dir.as_ref().canonicalize().unwrap();
        info!("libdistinst: Configuring on {}", mount_dir.display());
        let configure_dir = TempDir::new_in(mount_dir.join("tmp"), "distinst")?;
        let configure = configure_dir.path().join("configure.sh");

        {
            // Write the installer's intallation script to the chroot's temporary directory.
            info!("libdistinst: writing /tmp/configure.sh");
            let mut file = File::create(&configure)?;
            file.write_all(include_bytes!("scripts/configure.sh"))?;
            file.sync_all()?;
        }

        callback(15);

        {
            // Ubuntu's LVM auto-detection doesn't seem to work for activating root volumes.
            info!("libdistinst: applying LVM initramfs autodetect workaround");
            fs::create_dir_all(mount_dir.join("etc/initramfs-tools/scripts/local-top/"))?;
            let lvm_fix = mount_dir.join("etc/initramfs-tools/scripts/local-top/lvm-workaround");
            let mut file = File::create(&lvm_fix)?;
            file.write_all(include_bytes!("scripts/lvm-workaround.sh"))?;
            file.set_permissions(Permissions::from_mode(0o1755))?;
            file.sync_all()?;
        }

        callback(30);

        {
            // Generate and write all of the file system mount targets to the new install.
            info!("libdistinst: writing /etc/fstab");
            let fstab = mount_dir.join("etc/fstab");
            let mut file = File::create(&fstab)?;
            file.write_all(FSTAB_HEADER)?;
            file.write_all(disks.generate_fstab().as_bytes())?;
            file.sync_all()?;
        }

        callback(45);

        {
            // Do the same for generating any encryption entries needed by the install.
            info!("libdistinst: writing /etc/crypttab");
            let crypttab = mount_dir.join("etc/crypttab");
            let mut file = File::create(&crypttab)?;
            file.write_all(disks.generate_crypttab().as_bytes())?;
            file.sync_all()?;
        }

        callback(60);

        {
            info!(
                "libdistinst: chrooting into target on {}",
                mount_dir.display()
            );
            let mut chroot = Chroot::new(&mount_dir)?;
            let configure_chroot = configure.strip_prefix(&mount_dir).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Path::strip_prefix failed: {}", err),
                )
            })?;

            callback(75);

            let root_entry = {
                info!("libdistinst: retrieving root partition");
                disks
                    .get_physical_devices()
                    .iter()
                    .flat_map(|disk| disk.partitions.iter())
                    .filter_map(|part| part.get_block_info())
                    .find(|entry| entry.mount() == "/")
                    .or(disks
                        .get_logical_devices()
                        .iter()
                        .flat_map(|disk| disk.partitions.iter())
                        .filter_map(|part| part.get_block_info())
                        .find(|entry| entry.mount() == "/"))
                    .ok_or(io::Error::new(
                        io::ErrorKind::Other,
                        "root partition not found",
                    ))?
            };

            let install_pkgs: &[&str] = match bootloader {
                Bootloader::Bios => &["grub-pc"],
                Bootloader::Efi => &["kernelstub"],
            };

            info!(
                "libdistinst: will install {:?} bootloader packages",
                install_pkgs
            );

            let args = {
                let mut args = Vec::new();

                // Clear existing environment
                args.push("-i".to_string());

                // Set hostname to be set
                args.push(format!("HOSTNAME={}", config.hostname));

                // Set language to config setting
                args.push(format!("LANG={}", config.lang));

                // Set preferred keyboard layout
                args.push(format!("KBD={}", config.keyboard));

                // Set root UUID
                args.push(format!("ROOT_UUID={}", root_entry.uuid.to_str().unwrap()));

                // Run configure script with bash
                args.push("bash".to_string());

                // Path to configure script in chroot
                args.push(configure_chroot.to_str().unwrap().to_string());

                for pkg in install_pkgs {
                    // Install bootloader packages
                    args.push(pkg.to_string());
                }

                for pkg in remove_pkgs {
                    // Remove installer packages
                    args.push(format!("-{}", pkg.as_ref()));
                }

                args
            };

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

    /// Installs and configures the boot loader after it has been configured.
    fn bootloader<F: FnMut(i32)>(
        disks: &Disks,
        mount_dir: &Path,
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
            let efi_path = {
                let chroot = mount_dir.as_os_str().as_bytes();
                let mut target_mount: Vec<u8> = if chroot[chroot.len() - 1] == b'/' {
                    chroot.to_owned()
                } else {
                    let mut temp = chroot.to_owned();
                    temp.push(b'/');
                    temp
                };

                target_mount.extend_from_slice(b"boot/efi/");
                PathBuf::from(OsString::from_vec(target_mount))
            };

            // Also ensure that the /boot/efi directory is created.
            if efi_opt.is_some() {
                fs::create_dir_all(&efi_path)?;
            }

            {
                let mut chroot = Chroot::new(mount_dir)?;

                match bootloader {
                    Bootloader::Bios => {
                        let status = chroot.command(
                            "grub-install",
                            &[
                                // Recreate device map
                                "--recheck".into(),
                                // Install for BIOS
                                "--target=i386-pc".into(),
                                // Install to the bootloader_dev device
                                bootloader_dev.to_str().unwrap().to_owned(),
                            ],
                        )?;

                        if !status.success() {
                            return Err(io::Error::new(
                                io::ErrorKind::Other,
                                format!("grub-install failed with status: {}", status),
                            ));
                        }
                    }
                    Bootloader::Efi => {
                        let status = chroot.command(
                            "bootctl",
                            &[
                                // Install systemd-boot
                                "install",
                                // Provide path to ESP
                                "--path=/boot/efi",
                                // Do not set EFI variables
                                // TODO: Remove this option
                                "--no-variables",
                            ],
                        )?;

                        if !status.success() {
                            return Err(io::Error::new(
                                io::ErrorKind::Other,
                                format!("bootctl failed with status: {}", status),
                            ));
                        }
                    }
                }

                chroot.unmount(false)?;
            }
        }

        callback(100);

        Ok(())
    }

    /// The user will use this method to hand off installation tasks to distinst.
    ///
    /// The `disks` field contains all of the disks configuration information that will be
    /// applied before installation. The `config` field provides configuration details that
    /// will be applied when configuring the new installation.
    pub fn install(&mut self, mut disks: Disks, config: &Config) -> io::Result<()> {
        if !hostname::is_valid(&config.hostname) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "hostname is not valid",
            ));
        }

        let bootloader = Bootloader::detect();
        disks.verify_partitions(bootloader)?;

        let mut status = Status {
            step:    Step::Init,
            percent: 0,
        };

        macro_rules! apply_step {
            // When a step is provided, that step will be set. This branch will then invoke a
            // second call to the macro, which will execute the branch below this one.
            ($step:expr, $msg:expr, $action:expr) => {{
                unsafe { libc::sync(); }

                if KILL_SWITCH.load(Ordering::SeqCst) {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "process killed"));
                }

                status.step = $step;
                status.percent = 0;
                self.emit_status(&status);

                apply_step!($msg, $action);
            }};
            // When a step is not provided, the program will simply
            ($msg:expr, $action:expr) => {{
                info!("libdistinst: starting {} step", $msg);
                match $action {
                    Ok(value) => value,
                    Err(err) => {
                        error!("{} error: {}", $msg, err);
                        let error = Error {
                            step: status.step,
                            err:  err,
                        };
                        self.emit_error(&error);
                        return Err(error.err);
                    }
                }
            }};
        }

        macro_rules! percent {
            () => {
                |percent| {
                    status.percent = percent;
                    self.emit_status(&status);
                }
            };
        }

        info!("libdistinst: installing {:?} with {:?}", config, bootloader);
        self.emit_status(&status);

        let (squashfs, remove_pkgs) = apply_step!("initializing", {
            Installer::initialize(&mut disks, config, percent!())
        });

        apply_step!(Step::Partition, "partitioning", {
            Installer::partition(&mut disks, percent!())
        });

        // Mount the temporary directory, and all of our mount targets.
        const CHROOT_ROOT: &str = "distinst";
        info!(
            "libdistinst: mounting temporary chroot directory at {}",
            CHROOT_ROOT
        );

        {
            let mount_dir = TempDir::new(CHROOT_ROOT)?;

            {
                let mut mounts = Installer::mount(&disks, mount_dir.path())?;

                if PARTITIONING_TEST.load(Ordering::SeqCst) {
                    info!("libdistinst: PARTITION_TEST enabled: exiting before unsquashing");
                    return Ok(());
                }

                apply_step!(Step::Extract, "extraction", {
                    Installer::extract(squashfs.as_path(), mount_dir.path(), percent!())
                });

                apply_step!(Step::Configure, "configuration", {
                    Installer::configure(
                        &disks,
                        mount_dir.path(),
                        bootloader,
                        &config,
                        &remove_pkgs,
                        percent!(),
                    )
                });

                apply_step!(Step::Bootloader, "bootloader", {
                    Installer::bootloader(&disks, mount_dir.path(), bootloader, percent!())
                });

                mounts.unmount(false)?;
            }

            mount_dir.close()?;
        }

        Ok(())
    }

    /// Get a list of disks, skipping loopback devices
    ///
    /// ```ignore,rust
    /// use distinst::Installer;
    /// let installer = Installer::new();
    /// let disks = installer.disks().unwrap();
    /// ```
    pub fn disks(&self) -> io::Result<Disks> {
        info!("libdistinst: probing disks on system");
        Disks::probe_devices()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{}", err)))
    }
}
