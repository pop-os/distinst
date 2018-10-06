use envfile::EnvFile;
use disk::Disks;
use libc;
use os_release::OsRelease;
use std::fs::{self, Permissions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempdir::TempDir;
use distribution;
use Config;
use rayon;
use super::*;
use process::ChrootConfigurator;
use {INSTALL_HARDWARE_SUPPORT, misc, hardware_support};

use process::Chroot;
use process::external::remount_rw;

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
        );+
    }
}

pub fn configure<P: AsRef<Path>, S: AsRef<str>, F: FnMut(i32)>(
    disks: &Disks,
    mount_dir: P,
    config: &Config,
    iso_os_release: &OsRelease,
    remove_pkgs: &[S],
    mut callback: F,
) -> io::Result<()> {
    let mount_dir = mount_dir.as_ref().canonicalize().unwrap();
    info!("Configuring on {}", mount_dir.display());
    let configure_dir = TempDir::new_in(mount_dir.join("tmp"), "distinst")?;

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
        file_create!(
            lvm_fix,
            0o1755,
            [include_bytes!("../../scripts/lvm-workaround.sh")]
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
        let mut b: io::Result<()> = Ok(());
        let mut c: io::Result<()> = Ok(());
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
        map_errors! {
            b => "lvm autodetection error";
            c => "failed to generate fstab / crypttab"
        }

        disable_nvidia?
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

        let (retain, lang_output) = rayon::join(
            // Get packages required by this disk configuration.
            || distribution::debian::get_required_packages(&disks, iso_os_release),
            // Attempt to run the check-language-support external command.
            || distribution::debian::check_language_support(&config.lang, &chroot)
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
                    None => &[]
                }
            }
            None => &[]
        };

        // Filter the discovered language packs and retained packages from the remove list.
        let remove = remove_pkgs.into_iter()
            .map(AsRef::as_ref)
            .filter(|pkg| !lang_packs.iter().any(|x| pkg == x) && !retain.contains(&pkg))
            .collect::<Vec<&str>>();

        callback(35);

        // Add the retained packages to the list of packages to be installed.
        // There are some packages that Ubuntu will still remove even if they've been removed from the removal list.
        install_pkgs.extend_from_slice(&retain);

        // TODO: use a macro to make this more manageable.
        let chroot = ChrootConfigurator::new(chroot);

        let mut hostname = Ok(());
        let mut hosts = Ok(());
        let mut machine_id = Ok(());
        let mut netresolv = Ok(());
        let mut locale = Ok(());
        let mut apt_install = Ok(());
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
            // Apt takes so long that it needs to run by itself.
            s.spawn(|_| {
                apt_install = chroot.cdrom_add()
                    .and_then(|_| chroot.apt_install(&install_pkgs))
                    .and_then(|_| chroot.cdrom_disable());
            });
        });

        map_errors! {
            hostname => "failed to write hostname";
            hosts => "failed to write hosts";
            machine_id => "failed to write unique machine id";
            netresolv => "failed to link netresolve";
            locale => "failed to generate locales";
            apt_install => "failed to install packages";
            etc_cleanup => "failed to remove pre-existing files in /etc";
            kernel_copy => "failed to copy kernel from casper to chroot"
        }

        callback(70);

        let apt_remove = chroot.apt_remove(&remove);
        let recovery = chroot.recovery(
            config,
            &iso_os_release.name,
            &root_uuid,
            luks_uuid.as_ref().map_or("", |ref uuid| uuid.as_str())
        );

        map_errors! {
            apt_remove => "failed to remove packages";
            recovery => "failed to create recovery partition"
        }

        callback(75);

        chroot.bootloader().map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("failed to install bootloader: {}", why)
        ))?;

        callback(80);

        if disable_nvidia {
            chroot.disable_nvidia();
        }

        chroot.keyboard_layout(config).map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("failed to set keyboard layout: {}", why)
        ))?;
        callback(85);

        chroot.update_initramfs()?;
        callback(90);

        // Sync to the disk before unmounting
        unsafe { libc::sync(); }

        // Ensure that the cdrom binding is unmounted before the chroot.
        if let Some((cdrom_mount, cdrom_target)) = cdrom_mount {
            drop(cdrom_mount);
            fs::remove_dir(&cdrom_target);
        }

        drop(efivars_mount);
        callback(95);
    }

    configure_dir.close()?;
    callback(100);

    Ok(())
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
