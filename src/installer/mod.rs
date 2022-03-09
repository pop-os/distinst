pub mod bitflags;
pub mod traits;

mod conf;
mod state;

pub(crate) mod steps;

pub use self::{conf::RecoveryEnv, steps::Step};

use self::state::InstallerState;

use crate::auto::{
    delete_old_install, move_root, recover_root, remove_root, validate_backup_conditions,
    AccountFiles, Backup, ReinstallError,
};
use disk_types::{BlockDeviceExt, FileSystem};
use crate::disks::{Bootloader, Disks};
use crate::errors::IoContext;
use crate::external::luks::deactivate_logical_devices;
use crate::hostname;
use os_release::OsRelease;
use partition_identity::PartitionID;
use crate::squashfs;
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};
use tempdir::TempDir;
use crate::timezones::Region;
use crate::PARTITIONING_TEST;

pub const MODIFY_BOOT_ORDER: u8 = 0b01;
pub const INSTALL_HARDWARE_SUPPORT: u8 = 0b10;
pub const KEEP_OLD_ROOT: u8 = 0b100;
pub const RUN_UBUNTU_DRIVERS: u8 = 0b1000;

macro_rules! percent {
    ($steps:expr) => {
        |percent| {
            $steps.status.percent = percent;
            let status = $steps.status;
            $steps.emit_status(status);
        }
    };
}

/// Installer configuration
pub struct Config {
    /// Hostname to assign to the installed system.
    pub hostname:         String,
    /// The keyboard layout to use with the installed system (such as "us").
    pub keyboard_layout:  String,
    /// An optional keyboard model (such as "pc105") to define the keyboard's model.
    pub keyboard_model:   Option<String>,
    /// An optional variant of the keyboard (such as "dvorak").
    pub keyboard_variant: Option<String>,
    /// The UUID of the old root partition, for retaining user accounts.
    pub old_root:         Option<String>,
    /// The locale to use for the installed system.
    pub lang:             String,
    /// The file that contains a list of packages to remove.
    pub remove:           String,
    /// The archive (`tar` or `squashfs`) which contains the base system.
    pub squashfs:         String,
    /// Some flags to control the behavior of the installation.
    pub flags:            u8,
}

/// Credentials for creating a new user account.
#[derive(Clone)]
pub struct UserAccountCreate {
    pub username: String,
    pub realname: Option<String>,
    pub password: Option<String>,
    pub profile_icon: Option<String>,
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
    error_cb:         Option<Box<dyn FnMut(&Error)>>,
    status_cb:        Option<Box<dyn FnMut(&Status)>>,
    timezone_cb:      Option<Box<dyn FnMut() -> Region>>,
    user_creation_cb: Option<Box<dyn FnMut() -> UserAccountCreate>>,
}

impl Default for Installer {
    /// Create a new installer object
    ///
    /// ```ignore,rust
    /// use distinst::Installer;
    /// let installer = Installer::new();
    /// ```
    fn default() -> Self {
        Self {
            error_cb:         None,
            status_cb:        None,
            timezone_cb:      None,
            user_creation_cb: None,
        }
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
        Disks::probe_devices().with_context(|err| format!("disk probing error: {}", err))
    }

    /// The user will use this method to hand off installation tasks to distinst.
    ///
    /// The `disks` field contains all of the disks configuration information that will be
    /// applied before installation. The `config` field provides configuration details that
    /// will be applied when configuring the new installation.
    ///
    /// If `config.old_root` is set, then home at that location will be retained.
    pub fn install(&mut self, mut disks: Disks, config: &Config) -> io::Result<()> {
        debug!("Installing to {:#?}", disks);
        let mut recovery_conf = if Path::new("/cdrom/recovery.conf").exists() {
            Some(RecoveryEnv::new()?)
        } else {
            None
        };

        disks.remove_untouched_disks();
        let steps = &mut InstallerState::new(self);

        // If a btrfs root is defined without subvolumes, switch it to using subvolumes.
        let contains_home = disks.find_partition(Path::new("/home")).is_some();
        for partition in disks.get_partitions_mut() {
            if let Some(FileSystem::Btrfs) = partition.filesystem {
                if let Some(mount) = partition.mount_point.get(0) {
                    if Path::new("/") == mount {
                        partition.mount_point = Vec::new();
                        partition.subvolumes.insert(Path::new("/").into(), "@root".into());

                        if !contains_home {
                            partition.subvolumes.insert(Path::new("/home").into(), "@home".into());
                        }

                        break
                    }
                }
            }
        }

        debug!("AFTER {:#?}", disks);

        Self::backup(disks, config, steps, |mut disks, config, steps| {
            if !hostname::is_valid(&config.hostname) {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "hostname is not valid"));
            }

            let bootloader = Bootloader::detect();
            debug!("BEFORE VERIFY: {:#?}", disks);
            disks
                .verify_partitions(bootloader)
                .with_context(|err| format!("partition validation: {}", err))?;

            let (squashfs, remove_pkgs) = steps.apply(Step::Init, "initializing", |steps| {
                Installer::initialize(&mut disks, config, percent!(steps))
            })?;

            steps.apply(Step::Partition, "partitioning", |steps| {
                Installer::partition(&mut disks, percent!(steps))
            })?;

            // Mount the temporary directory, and all of our mount targets.
            info!("mounting temporary chroot directory at {}", Self::CHROOT_ROOT);

            let mount_dir = TempDir::new(Self::CHROOT_ROOT)
                .with_context(|err| format!("chroot root temp mount: {}", err))?;

            info!("mounting all targets to the temporary chroot");

            let mut mounts = disks
                .mount_all_targets(mount_dir.path())
                .with_context(|err| format!("mounting all targets: {}", err))?;

            if PARTITIONING_TEST.load(Ordering::SeqCst) {
                info!("PARTITION_TEST enabled: exiting before unsquashing");
                return Ok(());
            }

            let iso_os_release = steps.apply(Step::Extract, "extracting", |steps| {
                Installer::extract(squashfs.as_path(), mount_dir.path(), percent!(steps))
            })?;

            let timezone = steps.installer.timezone_cb.as_mut().map(|func| func());
            let user = steps.installer.user_creation_cb.as_mut().map(|func| func());

            steps.apply(Step::Configure, "configuring chroot", |steps| {
                Installer::configure(
                    recovery_conf.as_mut(),
                    &disks,
                    mount_dir.path(),
                    &config,
                    &iso_os_release,
                    timezone.as_ref(),
                    user.as_ref(),
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
                    percent!(steps),
                )
            })?;

            mounts.unmount(false).with_context(|err| format!("chroot unmount: {}", err))?;
            mount_dir.close().with_context(|err| format!("closing mount directory: {}", err))
        })?;

        let _ = deactivate_logical_devices();

        if let Some(conf) = recovery_conf.as_mut() {
            conf.remove("MODE");
            conf.write()?;
        }

        Ok(())
    }

    /// Create a backup of key data on the system, execute the given functi on, and then restore
    /// that backup. If a backup is not requested for the configuration, then it will just
    /// execute the given function.
    fn backup<F: FnMut(&mut Disks, &Config, &mut InstallerState) -> io::Result<()>>(
        mut disks: Disks,
        config: &Config,
        steps: &mut InstallerState,
        mut func: F,
    ) -> io::Result<()> {
        let account_files;
        let mut old_backup = None;

        let temp = Path::new("/tmp/distinst");
        let _ = std::fs::create_dir_all(temp);

        let backup = if let Some(ref old_root_uuid) = config.old_root {
            info!("installing while retaining home");

            let old_root = disks
                .get_partition_by_id(&PartitionID::new_uuid(old_root_uuid.clone()))
                .ok_or(ReinstallError::NoRootPartition)?;

            let (home, _) = disks
                .get_partition_with_target(Path::new("/home"))
                .map_or((old_root, true), |p| (p, false));

            if home.will_format() {
                return Err(ReinstallError::ReformattingHome.into());
            }

            let old_root_path = old_root.get_device_path();
            let old_root_fs = old_root.filesystem.ok_or(ReinstallError::NoFilesystem)?;

            let _mounts = disks.mount_all_targets(&temp)?;
            account_files = AccountFiles::new(temp)?;

            let backup = steps.apply(Step::Backup, "backing up", |steps| {
                let mut callback = percent!(steps);

                let backup = Backup::new(temp, &account_files)?;
                callback(25);

                validate_backup_conditions(&disks, &config.squashfs)?;
                callback(50);

                if config.flags & KEEP_OLD_ROOT != 0 {
                    move_root(temp)?;
                    old_backup = Some((old_root_path.to_path_buf(), old_root_fs));
                } else {
                    remove_root(temp)?;
                }

                callback(100);

                Ok(backup)
            })?;

            Some(backup)
        } else {
            None
        };

        // Do the destructive action of reinstalling the system.
        if let Err(why) = func(&mut disks, config, steps) {
            error!("errored while installing system: {}", why);

            let _mounts = disks.mount_all_targets(temp)?;
            recover_root(temp)?;

            return Err(why);
        }

        // Then restore the backup, if it exists.

        if let Some(backup) = backup {
            info!("applying backup");
            let _mounts = disks.mount_all_targets(temp)?;
            backup.restore(temp)?;

            if let Err(why) = delete_old_install(temp) {
                warn!("failed to delete old install: {}", why);
            }
        }

        info!("finishing job");
        let mut callback = percent!(steps);
        callback(100);

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

    /// Set the timezone callback
    pub fn set_timezone_callback<F: FnMut() -> Region + 'static>(&mut self, callback: F) {
        self.timezone_cb = Some(Box::new(callback));
    }

    pub fn set_user_callback<F: FnMut() -> UserAccountCreate + 'static>(&mut self, callback: F) {
        self.user_creation_cb = Some(Box::new(callback));
    }

    fn initialize<F: FnMut(i32)>(
        disks: &mut Disks,
        config: &Config,
        callback: F,
    ) -> io::Result<(PathBuf, Vec<String>)> {
        steps::initialize(disks, config, callback)
    }

    /// Apply all partitioning and formatting changes to the disks
    /// configuration specified.
    fn partition<F: FnMut(i32)>(disks: &mut Disks, callback: F) -> io::Result<()> {
        steps::partition(disks, callback)
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
        OsRelease::new_from(&mount_dir.join("etc/os-release")).with_context(|why| {
            format!("failed to parse /etc/os-release from extracted image: {}", why)
        })
    }

    /// Configures the new install after it has been extracted.
    fn configure<P: AsRef<Path>, S: AsRef<str>, F: FnMut(i32)>(
        recovery_conf: Option<&mut RecoveryEnv>,
        disks: &Disks,
        mount_dir: P,
        config: &Config,
        iso_os_release: &OsRelease,
        region: Option<&Region>,
        user: Option<&UserAccountCreate>,
        remove_pkgs: &[S],
        callback: F,
    ) -> io::Result<()> {
        steps::configure(
            recovery_conf,
            disks,
            mount_dir,
            config,
            iso_os_release,
            region,
            user,
            remove_pkgs,
            callback,
        )
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
        io::Error::new(io::ErrorKind::Other, format!("{}", why))
    }
}
