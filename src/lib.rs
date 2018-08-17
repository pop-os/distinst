//! A crate for installing Ubuntu distributions from a live squashfs

#![allow(unknown_lints)]

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate cascade;
extern crate dirs;
#[macro_use]
extern crate derive_new;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate fern;
extern crate itertools;
#[macro_use]
extern crate lazy_static;
extern crate libc;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate gettextrs;
extern crate iso3166_1;
extern crate isolang;
extern crate rand;
extern crate rayon;
extern crate raw_cpuid;
extern crate tempdir;
#[macro_use]
extern crate serde_derive;
extern crate serde_xml_rs;

use disk::external::{blockdev, cryptsetup_close, dmlist, encrypted_devices, pvs, remount_rw, vgactivate, vgdeactivate, CloseBy};
use disk::operations::FormatPartitions;
use itertools::Itertools;
use os_release::OsRelease;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::{self, Permissions};
use std::io::{self, BufRead, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering, ATOMIC_BOOL_INIT, ATOMIC_USIZE_INIT};
use std::thread::sleep;
use std::time::Duration;
use tempdir::TempDir;

pub use chroot::Chroot;
pub use command::Command;
pub use disk::mount::{BIND, Mount, Mounts};
pub use disk::{
    generate_unique_id, Bootloader, DecryptionError, Disk, DiskError, DiskExt, Disks,
    FileSystemType, LvmDevice, LvmEncryption, PartitionBuilder, PartitionError, PartitionFlag,
    PartitionInfo, PartitionTable, PartitionType, Sector, OS,
};
pub use misc::device_layout_hash;

pub mod auto;
pub mod chroot;
pub mod command;
mod configure;
mod disk;
mod distribution;
mod envfile;
mod hardware_support;
pub mod hostname;
pub mod locale;
mod misc;
pub mod os_release;
mod state;
mod squashfs;

use auto::{validate_before_removing, AccountFiles, Backup, ReinstallError};
use envfile::EnvFile;
use log::LevelFilter;
use self::state::InstallerState;

/// When set to true, this will stop the installation process.
pub static KILL_SWITCH: AtomicBool = ATOMIC_BOOL_INIT;

/// Force the installation to perform either a BIOS or EFI installation.
pub static FORCE_BOOTLOADER: AtomicUsize = ATOMIC_USIZE_INIT;

/// Exits before the unsquashfs step
pub static PARTITIONING_TEST: AtomicBool = ATOMIC_BOOL_INIT;

pub static NO_EFI_VARIABLES: AtomicBool = ATOMIC_BOOL_INIT;

/// Self-explanatory -- the fstab file will be generated with this header.
const FSTAB_HEADER: &[u8] = b"# /etc/fstab: static file system information.
#
# Use 'blkid' to print the universally unique identifier for a
# device; this may be used with UUID= as a more robust way to name devices
# that works even if disks are added and removed. See fstab(5).
#
# <file system>  <mount point>  <type>  <options>  <dump>  <pass>
";

pub const DEFAULT_ESP_SECTORS: u64 = 1_024_000;
pub const DEFAULT_RECOVER_SECTORS: u64 = 8_388_608;
pub const DEFAULT_SWAP_SECTORS: u64 = DEFAULT_RECOVER_SECTORS;

macro_rules! file_create {
    ($path:expr, $perm:expr, [ $($data:expr),+ ]) => {{
        let mut file = misc::create($path)?;
        $(file.write_all($data)?;)+
        file.set_permissions(Permissions::from_mode($perm))?;
        file.sync_all()?;
    }};

    ($path:expr, [ $($data:expr),+ ]) => {{
        let mut file = misc::create($path)?;
        $(file.write_all($data)?;)+
        file.sync_all()?;
    }};
}

/// Initialize logging with the fern logger
pub fn log<F: Fn(log::Level, &str) + Send + Sync + 'static>(
    callback: F,
) -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        // Exclude logs for crates that we use
        .level(LevelFilter::Off)
        // Include only the logs for this binary
        .level_for("distinst", LevelFilter::Debug)
        // This will be used by the front end for display logs in a UI
        .chain(fern::Output::call(move |record| {
            callback(record.level(), &format!("{}", record.args()))
        }))
        // Whereas this will handle displaying the logs to the terminal & a log file
        .chain({
            let mut logger = fern::Dispatch::new()
                .format(|out, message, record| {
                    out.finish(format_args!(
                        "[{}] {}: {}",
                        record.level(),
                        {
                            let target = record.target();
                            target.find(':').map_or(target, |pos| &target[..pos])
                        },
                        message
                    ))
                })
                .chain(std::io::stderr());

            match fern::log_file("/tmp/installer.log") {
                Ok(log) => logger = logger.chain(log),
                Err(why) => {
                    eprintln!("failed to create log file at /tmp/installer.log: {}", why);
                }
            };

            // If the home directory exists, add a log there as well.
            // If the Desktop directory exists within the home directory, write the logs there.
            if let Some(home) = dirs::home_dir() {
                let desktop = home.join("Desktop");
                let log = if desktop.is_dir() {
                    fern::log_file(&desktop.join("installer.log"))
                } else {
                    fern::log_file(&home.join("installer.log"))
                };

                match log {
                    Ok(log) => logger = logger.chain(log),
                    Err(why) => {
                        eprintln!("failed to set up logging for the home directory: {}", why);
                    }
                }
            }

            logger
        }).apply()?;

    Ok(())
}

pub fn deactivate_logical_devices() -> io::Result<()> {
    for luks_pv in encrypted_devices()? {
        info!("deactivating encrypted device named {}", luks_pv);
        if let Some(vg) = pvs()?.get(&PathBuf::from(["/dev/mapper/", &luks_pv].concat())) {
            match *vg {
                Some(ref vg) => {
                    vgdeactivate(vg).and_then(|_| cryptsetup_close(CloseBy::Name(&luks_pv)))?;
                },
                None => {
                    cryptsetup_close(CloseBy::Name(&luks_pv))?;
                },
            }
        }
    }

    Ok(())
}

/// Checks if the given name already exists as a device in the device map list.
pub fn device_map_exists(name: &str) -> bool {
    dmlist().ok().map_or(false, |list| list.contains(&name.into()))
}

/// Gets the minimum number of sectors required. The input should be in sectors, not bytes.
///
/// The number of sectors required is calculated through:
///
/// - The value in `/cdrom/casper/filesystem.size`
/// - The size of a default boot / esp partition
/// - The size of a default swap partition
/// - The size of a default recovery partition.
///
/// The input parameter will undergo a max comparison to the estimated minimum requirement.
pub fn minimum_disk_size(default: u64) -> u64 {
    let casper_size = misc::open("/cdrom/casper/filesystem.size")
        .ok()
        .and_then(|mut file| {
            let capacity = file.metadata().ok().map_or(0, |m| m.len());
            let mut buffer = String::with_capacity(capacity as usize);
            file.read_to_string(&mut buffer)
                .ok()
                .and_then(|_| buffer[..buffer.len() - 1].parse::<u64>().ok())
        })
        // Convert the number of bytes read into sectors required + 1
        .map(|bytes| (bytes / 512) + 1)
        .map_or(default, |size| size.max(default));

    casper_size + DEFAULT_ESP_SECTORS + DEFAULT_RECOVER_SECTORS + DEFAULT_SWAP_SECTORS
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

impl From<ReinstallError> for io::Error {
    fn from(why: ReinstallError) -> io::Error {
        io::Error::new(
            io::ErrorKind::Other,
            format!("{}", why)
        )
    }
}

pub const MODIFY_BOOT_ORDER: u8 = 0b01;
pub const INSTALL_HARDWARE_SUPPORT: u8 = 0b10;

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

impl Installer {
    const CHROOT_ROOT: &'static str = "distinst";

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

    fn initialize<F: FnMut(i32)>(
        disks: &mut Disks,
        config: &Config,
        retain: &[&str],
        mut callback: F,
    ) -> io::Result<(PathBuf, Vec<String>)> {
        info!("Initializing");

        let fetch_squashfs = || match Path::new(&config.squashfs).canonicalize() {
            Ok(squashfs) => if squashfs.exists() {
                info!("config.squashfs: found at {}", squashfs.display());
                Ok(squashfs)
            } else {
                error!("config.squashfs: supplied file does not exist");
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "invalid squashfs path",
                ))
            },
            Err(err) => {
                error!("config.squashfs: {}", err);
                Err(err)
            }
        };

        let fetch_packages = || {
            let mut remove_pkgs = Vec::new();
            {
                let file = match misc::open(&config.remove) {
                    Ok(file) => file,
                    Err(err) => {
                        error!("config.remove: {}", err);
                        return Err(err);
                    }
                };

                // Takes the locale, such as `en_US.UTF-8`, and changes it into `en`.
                let locale = match config.lang.find('_') {
                    Some(pos) => &config.lang[..pos],
                    None => match config.lang.find('.') {
                        Some(pos) => &config.lang[..pos],
                        None => &config.lang
                    }
                };

                // Attempt to run the check-language-support external command.
                let lang_output = distribution::debian::check_language_support(&locale)?;

                // Variable for storing a value that may be allocated.
                let lang_output_;

                // Collect a list of language packages to retain. If the command was not
                // found, an empty array will be returned.
                let lang_packs = match lang_output.as_ref() {
                    Some(output) => {
                        // Packages in the output are delimited with spaces.
                        // This is collected as a Cow<'_, str>.
                        let packages = output.split(|&x| x == b' ')
                            .map(|x| String::from_utf8_lossy(x))
                            .collect::<Vec<_>>();

                        match distribution::debian::get_dependencies_from_list(&packages) {
                            Some(dependencies) => {
                                lang_output_ = dependencies;
                                &lang_output_[..]
                            }
                            None => &[]
                        }
                    }
                    None => &[]
                };

                // Collects the packages that are to be removed from the install.
                for line_res in io::BufReader::new(file).lines() {
                    match line_res {
                        // Only add package if it is not contained within lang_packs.
                        Ok(line) => if !lang_packs.iter().any(|x| x == &line) && !retain.contains(&line.as_str()) {
                            remove_pkgs.push(line)
                        },
                        Err(err) => {
                            error!("config.remove: {}", err);
                            return Err(err);
                        }
                    }
                }
            }

            Ok(remove_pkgs)
        };

        let verify_disks = |disks: &Disks| {
            disks.verify_keyfile_paths()?;
            Ok(())
        };

        let mut res_a = Ok(());
        let mut res_b = Ok(Vec::new());
        let mut res_c = Ok(());
        let mut res_d = Ok(PathBuf::new());

        rayon::scope(|s| {
            s.spawn(|_| {
                // Deactivate any open logical volumes & close any encrypted partitions.
                if let Err(why) = disks.deactivate_device_maps() {
                    error!("device map deactivation error: {}", why);
                    res_a = Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("device map deactivation error: {}", why)
                    ));
                    return
                }

                // Unmount any mounted devices.
                if let Err(why) = disks.unmount_devices() {
                    error!("device unmount error: {}", why);
                    res_a = Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("device unmount error: {}", why)
                    ));
                    return
                }

                res_a = Ok(());
            });

            s.spawn(|_| res_b = fetch_packages());
            s.spawn(|_| res_c = verify_disks(disks));
            s.spawn(|_| res_d = fetch_squashfs());
        });

        let (remove_pkgs, squashfs) = res_a
            .and(res_c)
            .and(res_b)
            .and_then(|pkgs| res_d.map(|squashfs| (pkgs, squashfs)))?;

        let disks_ptr = &*disks as *const Disks;
        {
            let borrowed: &Disks = unsafe { &*disks_ptr };
            disks.physical.iter_mut().map(|disk| {
                // This will help us when we are testing in a dev environment.
                if disk.contains_mount("/", borrowed) {
                    return Ok(());
                }

                if let Err(why) = disk.unmount_all_partitions_with_target() {
                    error!("unable to unmount partitions");
                    return Err(io::Error::new(io::ErrorKind::Other, format!("{}", why)));
                }

                Ok(())
            }).collect::<io::Result<()>>()?;
        }

        callback(100);

        Ok((squashfs, remove_pkgs))
    }

    /// Apply all partitioning and formatting changes to the disks
    /// configuration specified.
    fn partition<F: FnMut(i32)>(disks: &mut Disks, mut callback: F) -> io::Result<()> {
        let (pvs_result, commit_result): (
            io::Result<BTreeMap<PathBuf, Option<String>>>,
            io::Result<()>
        ) = rayon::join(
            || {
                // This collection of physical volumes and their optional volume groups
                // will be used to obtain a list of volume groups associated with our
                // modified partitions.
                pvs().map_err(|why| io::Error::new(io::ErrorKind::Other, format!("{}", why)))
            },
            || {
                // Perform layout changes serially, due to libparted thread safety issues,
                // and collect a list of partitions to format which can be done in parallel.
                // Once partitions have been formatted in parallel, reload the disk configuration.
                let mut partitions_to_format = FormatPartitions(Vec::new());
                for disk in disks.get_physical_devices_mut() {
                    info!("{}: Committing changes to disk", disk.path().display());
                    if let Some(partitions) = disk.commit()
                        .map_err(|why| io::Error::new(
                            io::ErrorKind::Other,
                            format!("disk commit error: {}", why)
                        ))?
                    {
                        partitions_to_format.0.extend_from_slice(&partitions.0);
                    }
                }

                partitions_to_format.format()?;

                disks.physical.iter_mut()
                    .map(|disk| disk.reload().map_err(io::Error::from))
                    .collect()
            }
        );

        let pvs = commit_result.and(pvs_result)?;

        callback(25);

        // Utilizes the physical volume collection to generate a vector of volume
        // groups which we will need to deactivate pre-`blockdev`, and will be
        // reactivated post-`blockdev`.
        let vgs = disks.get_physical_partitions()
            .filter_map(|part| match pvs.get(&part.device_path) {
                Some(&Some(ref vg)) => Some(vg.clone()),
                _ => None,
            })
            .unique()
            .collect::<Vec<String>>();

        // Deactivate logical volumes so that blockdev will not fail.
        vgs.iter().map(|vg| vgdeactivate(vg)).collect::<io::Result<()>>()?;

        // Ensure that the logical volumes have had time to deactivate.
        sleep(Duration::from_secs(1));
        callback(50);

        // This is to ensure that everything's been written and the OS is ready to
        // proceed.
        disks.physical.par_iter().for_each(|disk| {
            let _ = blockdev(&disk.path(), &["--flushbufs", "--rereadpt"]);
        });

        // Give a bit of time to ensure that logical volumes can be re-activated.
        sleep(Duration::from_secs(1));
        callback(75);

        // Reactivate the logical volumes.
        vgs.iter().map(|vg| vgactivate(vg)).collect::<io::Result<()>>()?;

        let res = disks
            .commit_logical_partitions()
            .map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to commit logical partitions: {}", why)
            ));

        callback(100);
        res
    }

    /// Mount all target paths defined within the provided `disks`
    /// configuration.
    fn mount(disks: &Disks, chroot: &Path) -> io::Result<Mounts> {
        let targets = disks.get_partitions()
            .filter(|part| part.target.is_some() && part.filesystem.is_some());

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

                let fs = match target.filesystem.unwrap() {
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
                "mounting {} to {}, with {}",
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
        retain: &[&'static str],
        iso_os_release: &OsRelease,
        remove_pkgs: &[S],
        mut callback: F,
    ) -> io::Result<()> {
        let mount_dir = mount_dir.as_ref().canonicalize().unwrap();
        info!("Configuring on {}", mount_dir.display());
        let configure_dir = TempDir::new_in(mount_dir.join("tmp"), "distinst")?;

        // Pop!_OS does not need the retain workaround.
        let retain = if iso_os_release.name == "Pop!_OS" { &[] } else { retain };

        let install_pkgs = &mut cascade! {
            Vec::with_capacity(32);
            ..extend_from_slice(distribution::debian::get_bootloader_packages(&iso_os_release));
            ..extend_from_slice(retain);
        };

        callback(5);

        let lvm_autodetection = || {
            // Ubuntu's LVM auto-detection doesn't seem to work for activating root volumes.
            info!("applying LVM initramfs autodetect workaround");
            fs::create_dir_all(mount_dir.join("etc/initramfs-tools/scripts/local-top/"))?;
            let lvm_fix = mount_dir.join("etc/initramfs-tools/scripts/local-top/lvm-workaround");
            file_create!(
                lvm_fix,
                0o1755,
                [include_bytes!("scripts/lvm-workaround.sh")]
            );
            Ok(())
        };

        let generate_fstabs = || {
            let (crypttab, fstab) = disks.generate_fstabs();

            let (a, b) = rayon::join(
                || {
                    info!("writing /etc/crypttab");
                    file_create!(&mount_dir.join("etc/crypttab"), [crypttab.as_bytes()]);
                    Ok(())
                },
                || {
                    info!("writing /etc/fstab");
                    file_create!(
                        &mount_dir.join("etc/fstab"),
                        [FSTAB_HEADER, fstab.as_bytes()]
                    );
                    Ok(())
                }
            );

            a.and(b)
        };

        let disable_nvidia = {
            let mut b = Ok(());
            let mut c = Ok(());
            let mut disable_nvidia = Ok(false);

            rayon::scope(|s| {
                s.spawn(|_| b = lvm_autodetection());
                s.spawn(|_| c = generate_fstabs());
                s.spawn(|_| {
                    if config.flags & INSTALL_HARDWARE_SUPPORT != 0 {
                        hardware_support::append_packages(install_pkgs, &iso_os_release);
                    }

                    disable_nvidia = hardware_support::blacklist::disable_external_graphics(&mount_dir);
                });

            });

            callback(10);
            b.and(c).and(disable_nvidia)?
        };

        {
            info!(
                "chrooting into target on {}",
                mount_dir.display()
            );

            let chroot = cascade! {
                Chroot::new(&mount_dir)?;
                ..clear_envs(true);
                ..env("DEBIAN_FRONTEND", "noninteractive");
                ..env("HOME", "/root");
                ..env("LC_ALL", &config.lang);
                ..env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin");
            };

            let efivars_mount = mount_efivars(&mount_dir)?;
            let cdrom_mount = mount_cdrom(&mount_dir)?;
            let cdrom_target = cdrom_mount.as_ref().map(|x| x.dest().to_path_buf());

            callback(15);

            info!("retrieving root partition");
            let root_entry = disks
                .get_partitions()
                .filter_map(|part| part.get_block_info())
                .find(|entry| entry.mount() == "/")
                .ok_or_else(|| io::Error::new(
                    io::ErrorKind::Other,
                    "root partition not found",
                ))?;

            callback(20);

            let luks_uuid = misc::from_uuid(&root_entry.uuid)
                .and_then(|ref path| misc::resolve_to_physical(path.file_name().unwrap().to_str().unwrap()))
                .and_then(|ref path| misc::get_uuid(path))
                .and_then(|uuid| if uuid == root_entry.uuid { None } else { Some(uuid)});

            callback(25);

            let root_uuid = &root_entry.uuid;
            update_recovery_config(&mount_dir, &root_uuid, luks_uuid.as_ref().map(|x| x.as_str()))?;
            callback(30);

            let remove: Vec<&str> = remove_pkgs.iter()
                .filter(|pkg| !install_pkgs.contains(&pkg.as_ref()))
                .map(|x| x.as_ref())
                .collect();
            callback(35);

            // TODO: use a macro to make this more manageable.
            let mut chroot = configure::ChrootConfigure::new(chroot);
            let mut hostname = Ok(());
            let mut hosts = Ok(());
            let mut machine_id = Ok(());
            let mut netresolv = Ok(());
            let mut locale = Ok(());
            let mut apt_remove = Ok(());
            let mut etc_cleanup = Ok(());
            let mut kernel_copy = Ok(());

            rayon::scope(|s| {
                s.spawn(|_| {
                    hostname = chroot.hostname(&config.hostname);
                    hosts = chroot.hosts(&config.hostname);
                    machine_id = chroot.generate_machine_id();
                    netresolv = chroot.netresolve();
                    locale = chroot.generate_locale(&config.lang);
                    etc_cleanup = chroot.etc_cleanup();
                    kernel_copy = chroot.kernel_copy();
                });
                s.spawn(|_| {
                    apt_remove = chroot.apt_remove(&remove);
                });
            });

            hostname.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to write hostname: {}", why)
            ))?;

            hosts.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to write hosts: {}", why)
            ))?;

            machine_id.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to write unique machine id: {}", why)
            ))?;

            netresolv.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to link netresolv: {}", why)
            ))?;

            locale.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to generate locales: {}", why)
            ))?;

            apt_remove.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to remove packages: {}", why)
            ))?;

            etc_cleanup.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to remove pre-existing files in /etc: {}", why)
            ))?;

            callback(70);

            let (apt_install, recovery) = rayon::join(
                || {
                    chroot.cdrom_add()?;
                    chroot.apt_install(&install_pkgs)?;
                    chroot.cdrom_disable()
                },
                || {
                    chroot.recovery(
                        config,
                        &iso_os_release.name,
                        &root_uuid,
                        luks_uuid.as_ref().map_or("", |ref uuid| uuid.as_str())
                    )
                }
            );

            apt_install.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to install packages: {}", why)
            ))?;

            recovery.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to create recovery partition: {}", why)
            ))?;

            callback(75);

            chroot.bootloader().map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to install bootloader: {}", why)
            ))?;

            callback(80);

            if disable_nvidia {
                chroot.disable_nvidia();
            }

            chroot.update_initramfs()?;
            callback(85);

            chroot.keyboard_layout(config).map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("failed to set keyboard layout: {}", why)
            ))?;
            callback(90);

            // Ensure that the cdrom binding is unmounted before the chroot.
            drop(cdrom_mount);
            drop(efivars_mount);
            cdrom_target.map(|target| fs::remove_dir(&target));
            chroot.unmount(false)?;
            callback(95);
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
        config: &Config,
        iso_os_release: &OsRelease,
        mut callback: F,
    ) -> io::Result<()> {
        // Obtain the root device & partition, with an optional EFI device & partition.
        let ((root_dev, _root_part), boot_opt) = disks.get_base_partitions(bootloader);

        let mut efi_part_num = 0;

        let bootloader_dev = boot_opt.map_or(root_dev, |(dev, dev_part)| {
            efi_part_num = dev_part.number;
            dev
        });

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
            if bootloader == Bootloader::Efi && boot_opt.is_some() {
                fs::create_dir_all(&efi_path)?;
            }

            {
                let mut chroot = Chroot::new(mount_dir)?;
                let efivars_mount = mount_efivars(&mount_dir)?;

                match bootloader {
                    Bootloader::Bios => {
                        chroot.command(
                            "grub-install",
                            &[
                                // Recreate device map
                                "--recheck".into(),
                                // Install for BIOS
                                "--target=i386-pc".into(),
                                // Install to the bootloader_dev device
                                bootloader_dev.to_str().unwrap().to_owned(),
                            ],
                        ).run()?;
                    }
                    Bootloader::Efi => {
                        let name = &iso_os_release.name;
                        if &iso_os_release.name == "Pop!_OS" {
                            chroot.command(
                                "bootctl",
                                &[
                                    // Install systemd-boot
                                    "install",
                                    // Provide path to ESP
                                    "--path=/boot/efi",
                                    // Do not set EFI variables
                                    "--no-variables",
                                ][..],
                            ).run()?;
                        } else {
                            chroot.command(
                                "/usr/bin/env",
                                &[
                                    "bash",
                                    "-c",
                                    "echo GRUB_ENABLE_CRYPTODISK=y >> /etc/default/grub"
                                ]
                            ).run()?;

                            chroot.command(
                                "grub-install",
                                &[
                                    "--target=x86_64-efi",
                                    "--efi-directory=/boot/efi",
                                    &format!("--boot-directory=/boot/efi/EFI/{}", name),
                                    &format!("--bootloader={}", name),
                                    "--recheck",
                                ]
                            ).run()?;

                            chroot.command(
                                "grub-mkconfig",
                                &[ "-o", &format!("/boot/efi/EFI/{}/grub/grub.cfg", name)]
                            ).run()?;

                            chroot.command(
                                "update-initramfs",
                                &["-c", "-k", "all"]
                            ).run()?;
                        }

                        if config.flags & MODIFY_BOOT_ORDER != 0 {
                            let efi_part_num = efi_part_num.to_string();
                            let loader = if &iso_os_release.name == "Pop!_OS" {
                                "\\EFI\\systemd\\systemd-bootx64.efi".into()
                            } else {
                                format!("\\EFI\\{}\\grubx64.efi", name)
                            };

                            let args: &[&OsStr] = &[
                                "--create".as_ref(),
                                "--disk".as_ref(),
                                bootloader_dev.as_ref(),
                                "--part".as_ref(),
                                efi_part_num.as_ref(),
                                "--write-signature".as_ref(),
                                "--label".as_ref(),
                                iso_os_release.pretty_name.as_ref(),
                                "--loader".as_ref(),
                                loader.as_ref()
                            ][..];

                            chroot.command("efibootmgr", args).run()?;
                        }
                    }
                }

                drop(efivars_mount);
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
    ///
    /// If `config.old_root` is set, then home at that location will be retained.
    pub fn install(&mut self, mut disks: Disks, config: &Config) -> io::Result<()> {
        let account_files;

        let backup = if let Some(ref old_root_uuid) = config.old_root {
            info!("installing while retaining home");

            let current_disks =
                Disks::probe_devices().map_err(|why| ReinstallError::DiskProbe { why })?;
            let old_root = current_disks
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
            let old_root_fs = old_root
                .filesystem
                .ok_or_else(|| ReinstallError::NoFilesystem)?;
            let home_fs = home.filesystem.ok_or_else(|| ReinstallError::NoFilesystem)?;

            account_files = AccountFiles::new(old_root.get_device_path(), old_root_fs)?;
            let backup = Backup::new(home_path, home_fs, home_is_root, &account_files)?;

            validate_before_removing(&disks, &config.squashfs, home_path, home_fs)?;

            Some((backup, root_path, root_fs))
        } else {
            None
        };

        if !hostname::is_valid(&config.hostname) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "hostname is not valid",
            ));
        }

        let bootloader = Bootloader::detect();
        disks.verify_partitions(bootloader)?;

        let disk_support_flags = disks.get_support_flags();
        let retain = distribution::debian::get_required_packages(disk_support_flags);

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
            Installer::initialize(&mut disks, config, &retain, percent!(steps))
        })?;

        steps.apply(Step::Partition, "partitioning", |steps| {
            Installer::partition(&mut disks, percent!(steps))
        })?;

        // Mount the temporary directory, and all of our mount targets.
        info!("mounting temporary chroot directory at {}", Self::CHROOT_ROOT);

        // Stores the `OsRelease` parsed from the ISO's extracted image.
        let iso_os_release: OsRelease;

        {
            let mount_dir = TempDir::new(Self::CHROOT_ROOT)?;
            let mut mounts = Installer::mount(&disks, mount_dir.path())?;

            if PARTITIONING_TEST.load(Ordering::SeqCst) {
                info!("PARTITION_TEST enabled: exiting before unsquashing");
                return Ok(());
            }

            iso_os_release = steps.apply(Step::Extract, "extracting", |steps| {
                Installer::extract(squashfs.as_path(), mount_dir.path(), percent!(steps))
            })?;

            steps.apply(Step::Configure, "configuring chroot", |steps| {
                Installer::configure(
                    &disks,
                    mount_dir.path(),
                    &config,
                    &retain,
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
            mount_dir.close()?;
        }

        if let Some((backup, root_path, root_fs)) = backup {
            backup.restore(&root_path, root_fs)?;
        }

        let _ = deactivate_logical_devices();

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
        info!("probing disks on system");
        Disks::probe_devices()
            .map_err(|err| io::Error::new(
                io::ErrorKind::Other,
                format!("disk probing error: {}", err)
            ))
    }
}

fn update_recovery_config(mount: &Path, root_uuid: &str, luks_uuid: Option<&str>) -> io::Result<()> {
    fn remove_boot(mount: &Path, uuid: &str) -> io::Result<()> {
        for directory in mount.join("boot/efi/EFI").read_dir()? {
            let entry = directory?;
            let full_path = entry.path();
            if let Some(path) = entry.file_name().to_str() {
                if path.ends_with(uuid) {
                    info!("removing old boot files for {}", path);
                    fs::remove_dir_all(&full_path)?;
                }
            }
        }

        Ok(())
    }

    let recovery_path = Path::new("/cdrom/recovery.conf");
    if recovery_path.exists() {
        let recovery_conf = &mut EnvFile::new(recovery_path)?;
        let luks_value = luks_uuid.map_or("", |uuid| if root_uuid == uuid { "" } else { uuid});

        recovery_conf.update("LUKS_UUID", luks_value);

        remount_rw("/cdrom")
            .and_then(|_| {
                recovery_conf.update("OEM_MODE", "0");
                recovery_conf.get("ROOT_UUID").ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "no ROOT_UUID found in /cdrom/recovery.conf",
                    )
                })
            })
            .and_then(|old_uuid| remove_boot(mount, old_uuid))
            .and_then(|_| {
                recovery_conf.update("ROOT_UUID", root_uuid);
                recovery_conf.write()
            })?;
    }

    Ok(())
}

fn mount_cdrom(mount_dir: &Path) -> io::Result<Option<Mount>> {
    let cdrom_source = Path::new("/cdrom");
    let cdrom_target = mount_dir.join("cdrom");
    mount_bind_if_exists(&cdrom_source, &cdrom_target)
}

fn mount_efivars(mount_dir: &Path) -> io::Result<Option<Mount>> {
    if NO_EFI_VARIABLES.load(Ordering::Relaxed) {
        info!("was ordered to not mount the efivars directory");
        Ok(None)
    } else {
        let efivars_source = Path::new("/sys/firmware/efi/efivars");
        let efivars_target = mount_dir.join("sys/firmware/efi/efivars");
        mount_bind_if_exists(&efivars_source, &efivars_target)
    }
}

fn mount_bind_if_exists(source: &Path, target: &Path) -> io::Result<Option<Mount>> {
    if source.exists() {
        let _ = fs::create_dir_all(&target);
        Ok(Some(Mount::new(&source, &target, "none", BIND, None)?))
    } else {
        Ok(None)
    }
}
