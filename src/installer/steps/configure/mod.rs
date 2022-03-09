use crate::bootloader::Bootloader;
mod chroot_conf;
use self::chroot_conf::ChrootConfigurator;
use super::{mount_cdrom, mount_efivars};
use crate::installer::{conf::RecoveryEnv, steps::normalize_os_release_name};
use crate::chroot::Chroot;
use crate::distribution;
use crate::errors::*;
use crate::external::remount_rw;
use crate::hardware_support;
use crate::installer::traits::InstallerDiskOps;
use libc;
use crate::misc;
use os_release::OsRelease;
use partition_identity::PartitionID;
use rayon;
use std::{
    fs::{self, Permissions},
    io::{self, Write},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::Path,
};
use tempdir::TempDir;
use crate::timezones::Region;
use crate::Config;
use crate::UserAccountCreate;
use crate::INSTALL_HARDWARE_SUPPORT;
use crate::RUN_UBUNTU_DRIVERS;

/// Self-explanatory -- the fstab file will be generated with this header.
const FSTAB_HEADER: &[u8] = b"# /etc/fstab: static file system information.
#
# Use 'blkid' to print the universally unique identifier for a
# device; this may be used with UUID= as a more robust way to name devices
# that works even if disks are added and removed. See fstab(5).
#
# <file system>  <mount point>  <type>  <options>  <dump>  <pass>
";

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

#[macro_export]
macro_rules! map_errors {
    ( $( $var:expr => $value:expr );+ ) => {
        $(
            $var.map_err(|why| io::Error::new(
                io::ErrorKind::Other,
                format!("{}: {}", $value, why)
            ))?;
        )+
    }
}

pub fn configure<D: InstallerDiskOps, P: AsRef<Path>, S: AsRef<str>, F: FnMut(i32)>(
    recovery_conf: Option<&mut RecoveryEnv>,
    disks: &D,
    mount_dir: P,
    config: &Config,
    iso_os_release: &OsRelease,
    region: Option<&Region>,
    user: Option<&UserAccountCreate>,
    remove_pkgs: &[S],
    mut callback: F,
) -> io::Result<()> {
    let mount_dir = mount_dir.as_ref().canonicalize().unwrap();
    info!("Configuring on {}", mount_dir.display());
    let tpath = mount_dir.join("tmp");
    let configure_dir = TempDir::new_in(&tpath, "distinst")
        .with_context(|err| format!("creating tempdir at {:?}: {}", tpath, err))?;

    let install_pkgs = &mut cascade! {
        Vec::with_capacity(32);
        ..extend_from_slice(distribution::debian::get_bootloader_packages(&iso_os_release));
    };

    callback(5);

    let lvm_autodetection = || {
        // Ubuntu's LVM auto-detection doesn't seem to work for activating root volumes.
        info!("applying LVM initramfs autodetect workaround");
        fs::create_dir_all(mount_dir.join("etc/initramfs-tools/scripts/local-top/"))?;
        let lvm_fix = mount_dir.join("etc/initramfs-tools/scripts/local-top/lvm-workaround");
        file_create!(lvm_fix, 0o1755, [include_bytes!("../../../scripts/lvm-workaround.sh")]);
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
                file_create!(&mount_dir.join("etc/fstab"), [FSTAB_HEADER, fstab.as_bytes()]);
                Ok(())
            },
        );

        a.and(b)
    };

    let configure_graphics = {
        let mut b: io::Result<()> = Ok(());
        let mut c: io::Result<()> = Ok(());
        let mut configure_graphics = Ok(false);

        rayon::scope(|s| {
            s.spawn(|_| b = lvm_autodetection());
            s.spawn(|_| c = generate_fstabs());
            s.spawn(|_| {
                if config.flags & INSTALL_HARDWARE_SUPPORT != 0 {
                    hardware_support::append_packages(install_pkgs, &iso_os_release);
                }

                configure_graphics = hardware_support::switchable_graphics::configure_graphics(&mount_dir);
            });
        });

        callback(10);
        map_errors! {
            b => "lvm autodetection error";
            c => "failed to generate fstab / crypttab"
        }

        configure_graphics?
    };

    {
        info!("chrooting into target on {}", mount_dir.display());

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

        callback(15);

        let root_entry = disks.get_block_info_of("/")?;
        let _recovery_entry = disks.get_block_info_of("/recovery");

        callback(20);

        let luks_uuid = root_entry
            .uid
            .get_device_path()
            .and_then(|ref path| {
                misc::resolve_to_physical(path.file_name().unwrap().to_str().unwrap())
            })
            .and_then(PartitionID::get_uuid)
            .and_then(|uuid| if uuid == root_entry.uid { None } else { Some(uuid) });

        callback(25);

        let root_uuid = &root_entry.uid;
        if let Some(conf) = recovery_conf {
            update_recovery_config(
                conf,
                &mount_dir,
                &root_uuid.id,
                luks_uuid.as_ref().map(|x| x.id.as_str()),
            )?;
        }

        callback(30);

        let (retain, lang_output) = rayon::join(
            // Get packages required by this disk configuration.
            || distribution::debian::get_required_packages(disks, iso_os_release),
            // Attempt to run the check-language-support external command.
            || distribution::debian::check_language_support(&config.lang, &chroot),
        );

        let lang_output = lang_output?;

        // Variable for storing a value that may be allocated.
        let lang_output_;

        // Collect a list of language packages to retain. If the command was not
        // found, an empty array will be returned.
        let lang_packs = match lang_output.as_ref() {
            Some(output) => {
                // Packages in the output are delimited with spaces.
                // This is collected as a Cow<'_, str>.
                let packages = output.split_whitespace().collect::<Vec<_>>();

                match distribution::debian::get_dependencies_from_list(&packages) {
                    Some(dependencies) => {
                        lang_output_ = dependencies;
                        &lang_output_[..]
                    }
                    None => &[],
                }
            }
            None => &[],
        };

        // Add the retained packages to the list of packages to be installed.
        // There are some packages that Ubuntu will still remove even if they've been removed from
        // the removal list.
        install_pkgs.extend_from_slice(&retain);

        // Filter the discovered language packs and installed packages from the remove list.
        let mut remove = remove_pkgs
            .iter()
            .map(AsRef::as_ref)
            .filter(|pkg| !lang_packs.iter().any(|x| pkg == x) && !install_pkgs.contains(&pkg))
            .collect::<Vec<&str>>();

        // Remove incompatible bootloader packages
        match Bootloader::detect() {
            Bootloader::Bios => {
                if iso_os_release.name == "Pop!_OS" {
                    remove.push("kernelstub");
                }
            }
            Bootloader::Efi => (),
        }

        callback(35);

        // TODO: use a macro to make this more manageable.
        let chroot = ChrootConfigurator::new(chroot);

        chroot.initramfs_disable()?;

        let hostname = chroot.hostname(&config.hostname);
        let hosts = chroot.hosts(&config.hostname);
        let machine_id = chroot.generate_machine_id();
        let netresolv = chroot.netresolve();
        let locale = chroot.generate_locale(&config.lang);
        let kernel_copy = chroot.kernel_copy();

        let timezone = if let Some(tz) = region {
            chroot.timezone(tz)
        } else {
            Ok(())
        };

        let useradd = if let Some(ref user) = user {
            chroot.create_user(
                &user.username,
                user.password.as_deref(),
                user.realname.as_deref(),
                user.profile_icon.as_deref(),
            )
        } else {
            Ok(())
        };

        let apt_install = chroot
            .cdrom_add()
            .and_then(|_| chroot.apt_install(&install_pkgs))
            .and_then(|_| chroot.install_drivers(config.flags & RUN_UBUNTU_DRIVERS != 0))
            .and_then(|_| chroot.cdrom_disable());

        map_errors! {
            hostname => "error writing hostname";
            hosts => "error writing hosts";
            machine_id => "error writing unique machine id";
            netresolv => "error linking netresolve";
            locale => "error generating locales";
            apt_install => "error installing packages";
            kernel_copy => "error copying kernel from casper to chroot";
            timezone => "error setting timezone";
            useradd => "error creating user account"
        }

        callback(70);

        let apt_remove = chroot.apt_remove(&remove);
        let recovery = chroot.recovery(
            config,
            &normalize_os_release_name(&iso_os_release.name),
            &root_uuid.id,
            luks_uuid.as_ref().map_or("", |ref uuid| uuid.id.as_str()),
        );

        map_errors! {
            apt_remove => "error removing packages";
            recovery => "error creating recovery partition"
        }

        callback(75);

        chroot.bootloader(disks.rootflags().as_deref())
            .with_context(|why| format!("error installing bootloader: {}", why))?;

        callback(80);

        if configure_graphics {
            chroot.disable_nvidia_fallback();
        }

        chroot
            .keyboard_layout(config)
            .with_context(|why| format!("error setting keyboard layout: {}", why))?;
        callback(85);

        chroot.initramfs_reenable()?;

        callback(90);

        // Sync to the disk before unmounting
        unsafe {
            libc::sync();
        }

        // Ensure that the cdrom binding is unmounted before the chroot.
        if let Some((cdrom_mount, cdrom_target)) = cdrom_mount {
            drop(cdrom_mount);
            let _ = fs::remove_dir(&cdrom_target);
        }

        drop(efivars_mount);
        callback(95);
    }

    configure_dir.close()?;
    callback(100);

    Ok(())
}

fn update_recovery_config(
    recovery_conf: &mut RecoveryEnv,
    mount: &Path,
    root_uuid: &str,
    luks_uuid: Option<&str>,
) -> io::Result<()> {
    fn remove_boot(mount: &Path, uuid: &str) -> io::Result<()> {
        let efi_path = mount.join("boot/efi/EFI");
        let readdir = efi_path
            .read_dir()
            .with_context(|err| format!("error reading dir at {:?}: {}", efi_path, err))?;

        for directory in readdir {
            let entry =
                directory.with_context(|err| format!("bad entry in {:?}: {}", efi_path, err))?;
            let full_path = entry.path();
            if let Some(path) = entry.file_name().to_str() {
                if path.ends_with(uuid) {
                    fs::remove_dir_all(&full_path).with_context(|err| {
                        format!("error removing old boot files for {}: {}", path, err)
                    })?;
                }
            }
        }

        Ok(())
    }

    let recovery_path = Path::new("/cdrom/recovery.conf");
    if recovery_path.exists() {
        let luks_value = luks_uuid.map_or("", |uuid| if root_uuid == uuid { "" } else { uuid });
        recovery_conf.update("LUKS_UUID", luks_value);

        remount_rw("/cdrom")
            .with_context(|err| format!("could not remount /cdrom as rw: {}", err))
            .and_then(|_| {
                recovery_conf.update("OEM_MODE", "0");
                recovery_conf
                    .get("ROOT_UUID")
                    .into_io_result(|| "no ROOT_UUID found in /cdrom/recovery.conf")
            })
            .map(|old_uuid| {
                let res = remove_boot(mount, old_uuid).with_context(|err| {
                    format!("unable to remove an older boot from recovery.conf: {}", err)
                });

                if let Err(why) = res {
                    warn!("{}", why);
                }
            })
            .and_then(|_| {
                recovery_conf.update("ROOT_UUID", root_uuid);
                recovery_conf
                    .write()
                    .with_context(|err| format!("error writing recovery conf: {}", err))
            })?;
    }

    Ok(())
}
