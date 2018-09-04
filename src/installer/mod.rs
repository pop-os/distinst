mod state;
mod steps;

use {PARTITIONING_TEST, deactivate_logical_devices, hostname, squashfs};
use auto::{recover_root, remove_root, move_root, validate_backup_conditions, AccountFiles, Backup, ReinstallError};
use disk::{Bootloader, Disks};
use os_release::OsRelease;
use self::state::InstallerState;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tempdir::TempDir;

pub use process::Chroot;
pub use process::Command;
pub use mnt::{BIND, Mount, Mounts};
pub use self::steps::Step;

pub const MODIFY_BOOT_ORDER: u8 = 0b01;
pub const INSTALL_HARDWARE_SUPPORT: u8 = 0b10;
pub const KEEP_OLD_ROOT: u8 = 0b100;

/// Installer configuration
#[derive(Debug)]
pub struct Config {
    /// Hostname to assign to the installed system.
    pub hostname: String,
    /// The keyboard layout to use with the installed system (such as "us").
    pub keyboard_layout: String,
    /// An optional keyboard model (such as "pc105") to define the keyboard's model.
    pub keyboard_model: Option<String>,
    /// An optional variant of the keyboard (such as "dvorak").
    pub keyboard_variant: Option<String>,
    /// The UUID of the old root partition, for retaining user accounts.
    pub old_root: Option<String>,
    /// The locale to use for the installed system.
    pub lang: String,
    /// The file that contains a list of packages to remove.
    pub remove: String,
    /// The archive (`tar` or `squashfs`) which contains the base system.
    pub squashfs: String,
    /// Some flags to control the behavior of the installation.
    pub flags: u8,
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

impl Default for Installer {
    /// Create a new installer object
    ///
    /// ```ignore,rust
    /// use distinst::Installer;
    /// let installer = Installer::new();
    /// ```
    fn default() -> Installer {
        Self { error_cb: None, status_cb: None }
    }
}

impl Installer {
    const CHROOT_ROOT: &'static str = "distinst";

    /// Get a list of disks, skipping loopback devices
    ///
    /// ```ignore,rust
    /// use distinst::Installer;
    /// let installer = Installer::new();
    /// let disks = installer.disks().unwrap();
    /// ```
    pub fn disks(&self) -> io::Result<Disks> {
        info!("probing disks on system");
        Disks::probe_devices()
            .map_err(|err| io::Error::new(
                io::ErrorKind::Other,
                format!("disk probing error: {}", err)
            ))
    }

    /// The user will use this method to hand off installation tasks to distinst.
    ///
    /// The `disks` field contains all of the disks configuration information that will be
    /// applied before installation. The `config` field provides configuration details that
    /// will be applied when configuring the new installation.
    ///
    /// If `config.old_root` is set, then home at that location will be retained.
    pub fn install(&mut self, disks: Disks, config: &Config) -> io::Result<()> {
        Self::backup(disks, config, |mut disks, config| {
            if !hostname::is_valid(&config.hostname) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "hostname is not valid",
                ));
            }

            let bootloader = Bootloader::detect();
            disks.verify_partitions(bootloader)?;

            info!("installing {:?} with {:?}", config, bootloader);

            let steps = &mut InstallerState::new(self);

            macro_rules! percent {
                ($steps:expr) => {
                    |percent| {
                        $steps.status.percent = percent;
                        let status = $steps.status;
                        $steps.emit_status(status);
                    }
                }
            }

            let (squashfs, remove_pkgs) = steps.apply(Step::Init, "initializing", |steps| {
                Installer::initialize(&mut disks, config, percent!(steps))
            })?;

            steps.apply(Step::Partition, "partitioning", |steps| {
                Installer::partition(&mut disks, percent!(steps))
            })?;

            // Mount the temporary directory, and all of our mount targets.
            info!("mounting temporary chroot directory at {}", Self::CHROOT_ROOT);

            let mount_dir = TempDir::new(Self::CHROOT_ROOT)?;
            let mut mounts = Installer::mount(&disks, mount_dir.path())?;

            if PARTITIONING_TEST.load(Ordering::SeqCst) {
                info!("PARTITION_TEST enabled: exiting before unsquashing");
                return Ok(());
            }

            let iso_os_release = steps.apply(Step::Extract, "extracting", |steps| {
                Installer::extract(squashfs.as_path(), mount_dir.path(), percent!(steps))
            })?;

            steps.apply(Step::Configure, "configuring chroot", |steps| {
                Installer::configure(
                    &disks,
                    mount_dir.path(),
                    &config,
                    &iso_os_release,
                    &remove_pkgs,
                    percent!(steps),
                )
            })?;

            steps.apply(Step::Bootloader, "configuring bootloader", |steps| {
                Installer::bootloader(
                    &disks,
                    mount_dir.path(),
                    bootloader,
                    &config,
                    &iso_os_release,
                    percent!(steps)
                )
            })?;

            mounts.unmount(false)?;
            mount_dir.close()
        })?;

        let _ = deactivate_logical_devices();

        Ok(())
    }

    /// Create a backup of key data on the system, execute the given functi on, and then restore
    /// that backup. If a backup is not requested for the configuration, then it will just
    /// execute the given function.
    fn backup<F: FnMut(Disks, &Config) -> io::Result<()>>(
        disks: Disks,
        config: &Config,
        mut func: F
    ) -> io::Result<()> {
        let account_files;
        let mut old_backup = None;

        let backup = if let Some(ref old_root_uuid) = config.old_root {
            info!("installing while retaining home");

            let old_root = disks
                .get_partition_by_uuid(old_root_uuid)
                .ok_or(ReinstallError::NoRootPartition)?;

            let new_root = disks
                .get_partition_with_target(Path::new("/"))
                .ok_or(ReinstallError::NoRootPartition)?;

            let (home, home_is_root) = disks
                .get_partition_with_target(Path::new("/home"))
                .map_or((old_root, true), |p| (p, false));

            if home.will_format() {
                return Err(ReinstallError::ReformattingHome.into());
            }

            let home_path = home.get_device_path();
            let root_path = new_root.get_device_path().to_path_buf();
            let root_fs = new_root
                .filesystem
                .ok_or_else(|| ReinstallError::NoFilesystem)?;
            let old_root_path = old_root.get_device_path();
            let old_root_fs = old_root
                .filesystem
                .ok_or_else(|| ReinstallError::NoFilesystem)?;
            let home_fs = home.filesystem.ok_or_else(|| ReinstallError::NoFilesystem)?;

            account_files = AccountFiles::new(old_root_path, old_root_fs)?;
            let backup = Backup::new(home_path, home_fs, home_is_root, &account_files)?;
            validate_backup_conditions(&disks, &config.squashfs)?;

            if config.flags & KEEP_OLD_ROOT != 0 {
                move_root(old_root_path, old_root_fs)?;
            } else {
                remove_root(old_root_path, old_root_fs)?;
                old_backup = Some((old_root_path.to_path_buf(), old_root_fs));
            }

            Some((backup, root_path, root_fs))
        } else {
            None
        };

        // Do the destructive action of reinstalling the system.
        if let Err(why) = func(disks, config) {
            error!("errored while installing system: {}", why);

            if let Some((path, fs)) = old_backup {
                recover_root(&path, fs)?;
            }

            return Err(why);
        }

        // Then restore the backup, if it exists.
        if let Some((backup, root_path, root_fs)) = backup {
            backup.restore(&root_path, root_fs)?;
        }

        Ok(())
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
    pub fn emit_status(&mut self, status: Status) {
        if let Some(ref mut cb) = self.status_cb {
            cb(&status);
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

    fn initialize<F: FnMut(i32)>(disks: &mut Disks, config: &Config, callback: F)
        -> io::Result<(PathBuf, Vec<String>)>
    {
        steps::initialize(disks, config, callback)
    }

    /// Apply all partitioning and formatting changes to the disks
    /// configuration specified.
    fn partition<F: FnMut(i32)>(disks: &mut Disks, callback: F) -> io::Result<()> {
        steps::partition(disks, callback)
    }

    /// Mount all target paths defined within the provided `disks`
    /// configuration.
    fn mount(disks: &Disks, chroot: &Path) -> io::Result<Mounts> {
        steps::mount(disks, chroot)
    }

    /// Extracts the squashfs image into the new install, and then gets the os-release data.
    ///
    /// We get the os-release data here because the host that is installing the image may differ
    /// from the image that is being installed, and thus may be a completely different distro.
    fn extract<P: AsRef<Path>, F: FnMut(i32)>(
        squashfs: P,
        mount_dir: P,
        callback: F,
    ) -> io::Result<OsRelease> {
        info!("Extracting {}", squashfs.as_ref().display());
        let mount_dir = mount_dir.as_ref();
        squashfs::extract(squashfs, mount_dir, callback)?;
        OsRelease::new_from(&mount_dir.join("etc/os-release"))
            .map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to parse /etc/os-release from extracted image: {}", why)
            ))
    }

    /// Configures the new install after it has been extracted.
    fn configure<P: AsRef<Path>, S: AsRef<str>, F: FnMut(i32)>(
        disks: &Disks,
        mount_dir: P,
        config: &Config,
        iso_os_release: &OsRelease,
        remove_pkgs: &[S],
        callback: F,
    ) -> io::Result<()> {
        steps::configure(disks, mount_dir, config, iso_os_release, remove_pkgs, callback)
    }

    /// Installs and configures the boot loader after it has been configured.
    fn bootloader<F: FnMut(i32)>(
        disks: &Disks,
        mount_dir: &Path,
        bootloader: Bootloader,
        config: &Config,
        iso_os_release: &OsRelease,
        callback: F,
    ) -> io::Result<()> {
        steps::bootloader(disks, mount_dir, bootloader, config, iso_os_release, callback)
    }
}

impl From<ReinstallError> for io::Error {
    fn from(why: ReinstallError) -> io::Error {
        io::Error::new(
            io::ErrorKind::Other,
            format!("{}", why)
        )
    }
}
