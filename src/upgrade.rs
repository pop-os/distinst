use crate::os_release::OS_RELEASE;
use apt_cli_wrappers::AptUpgradeEvent;
use auto::{InstallOption, InstallOptionError, RecoveryOption};
use chroot::{Command, SystemdNspawn};
use disks::Disks;
use envfile::EnvFile;
use errors::IoContext;
use external::remount_rw;
use installer::steps::mount_efivars;
use std::{io, path::Path, process::Stdio};
use systemd_boot_conf::SystemdBootConf;
use tempdir::TempDir;

const TARGET: &str = "/upgrade";

#[derive(Debug, Error)]
pub enum UpgradeError {
    #[error(display = "attempted to recover from errors, but failed: {}", _0)]
    AttemptFailed(io::Error),
    #[error(display = "failed to mount file systems to chroot: {}", _0)]
    ChrootMount(io::Error),
    #[error(display = "failed to create temporary chroot mount directory: {}", _0)]
    ChrootTempCreate(io::Error),
    #[error(display = "failed to configure disk(s): {}", _0)]
    Configure(InstallOptionError),
    #[error(display = "failed to rename EFI entry: {}", _0)]
    EfiEntryRename(io::Error),
    #[error(display = "failed to mount efivars directory: _0")]
    EfiVars(io::Error),
    #[error(display = "failed to mount $CHROOT/etc to /etc: {}", _0)]
    EtcMount(io::Error),
    #[error(display = "failed to find the Pop_OS-current entry in systemd-boot's efi loaders")]
    MissingCurrentEntry,
    #[error(display = "attempted an upgrade, but the upgrade mode was not set")]
    ModeNotSet,
    #[error(display = "systemd-boot loader conf error: {}", _0)]
    SystemdBootConf(systemd_boot_conf::Error),
    #[error(display = "systemd-boot loader conf write error: {}", _0)]
    SystemdBootConfWrite(systemd_boot_conf::Error),
    #[error(display = "failed to remove upgrade flag from recovery.conf: {}", _0)]
    UpgradeFlag(io::Error),
}

#[derive(Debug)]
pub enum UpgradeEvent<'a> {
    AttemptingRepair,
    AttemptingUpgrade,
    Autoremoving,
    DpkgInfo(&'a str),
    DpkgErr(&'a str),
    UpgradeInfo(&'a str),
    UpgradeErr(&'a str),
    PackageProcessing(&'a str),
    PackageProgress(u8),
    PackageSettingUp(&'a str),
    PackageUnpacking { package: &'a str, version: &'a str, over: &'a str },
    ResumingUpgrade,
}

/// Chroot into an existing install, and upgrade it to the next release.
pub fn upgrade<F: Fn(UpgradeEvent)>(
    disks: &mut Disks,
    option: &RecoveryOption,
    callback: F,
) -> Result<(), UpgradeError> {
    if !option.upgrade_mode {
        return Err(UpgradeError::ModeNotSet);
    }

    InstallOption::Upgrade(option).apply(disks).map_err(UpgradeError::Configure)?;

    prepare_mount(disks, move |chroot| attempt_upgrade(chroot, |event| callback(event)))
}

/// Allow the caller to attempt to resume an upgrade, after performing a manual recovery.
pub fn resume_upgrade<F: Fn(UpgradeEvent), R: Fn(&'static str)>(
    disks: &Disks,
    callback: F,
    repair: R,
) -> Result<(), UpgradeError> {
    prepare_mount(disks, move |chroot| {
        repair(TARGET);
        attempt_upgrade(chroot, |event| callback(event))
    })
}

fn attempt_upgrade<F: Fn(UpgradeEvent)>(
    chroot: &mut SystemdNspawn,
    callback: F,
) -> Result<(), UpgradeError> {
    callback(UpgradeEvent::AttemptingUpgrade);
    if let Err(why) = attempt(chroot, &callback) {
        error!("upgrade attempt failed: {}", why);
        return Err(UpgradeError::AttemptFailed(why));
    }

    disable_upgrade_flag().map_err(UpgradeError::UpgradeFlag)?;
    systemd_boot_entry_restore(TARGET)?;
    rename_efi_entry().map_err(UpgradeError::EfiEntryRename)?;

    Ok(())
}

fn disable_upgrade_flag() -> io::Result<()> {
    let recovery_conf = &mut EnvFile::new("/cdrom/recovery.conf")
        .with_context(|err| format!("error parsing envfile at /cdrom/recovery.conf: {}", err))?;

    remount_rw("/cdrom")
        .with_context(|err| format!("could not remount /cdrom as rw: {}", err))
        .and_then(|_| {
            recovery_conf.store.remove("UPGRADE");
            recovery_conf
                .write()
                .with_context(|err| format!("error writing recovery conf: {}", err))
        })
}

fn rename_efi_entry() -> io::Result<()> {
    const BOOT_MANAGER: &str = "efibootmgr";

    let boot_command = Command::new(BOOT_MANAGER)
        .run_with_stdout()
        .with_context(|err| format!("error getting output from efibootmgr: {}", err))?;

    let boot_current = boot_command
        .lines()
        .find(|line| line.trim_start().starts_with("BootCurrent"))
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "efibootmgr did not return BootCurrent line")
        })?;

    let boot_current_num = boot_current.split_whitespace().nth(1).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "efibootmgr BootCurrent line lacks bootnum")
    })?;

    let pretty_name = &OS_RELEASE.as_ref().unwrap().pretty_name;

    Command::new(BOOT_MANAGER)
        .args(&["-b", boot_current_num, "-B"])
        .run()
        .with_context(|err| format!("efibootmgr failed to delete current bootnum: {}", err))?;

    Command::new(BOOT_MANAGER)
        .args(&["-c", "-L", pretty_name.as_str()])
        .run()
        .with_context(|err| format!("efibootmgr failed to recreate current entry: {}", err))?;

    Ok(())
}

fn systemd_boot_entry_restore<P: AsRef<Path>>(base: P) -> Result<(), UpgradeError> {
    let mut systemd_boot_conf = SystemdBootConf::new(base.as_ref().join("boot/efi"))
        .map_err(UpgradeError::SystemdBootConf)?;

    {
        info!("found the systemd-boot config -- searching for the current entry");
        let SystemdBootConf { ref entries, ref mut loader_conf, .. } = systemd_boot_conf;
        let current_entry = entries
            .iter()
            .find(|e| e.filename.to_lowercase() == "pop_os-current")
            .ok_or(UpgradeError::MissingCurrentEntry)?;

        loader_conf.default = Some(current_entry.filename.to_owned());
    }

    systemd_boot_conf.overwrite_loader_conf().map_err(UpgradeError::SystemdBootConfWrite)?;

    Ok(())
}

fn apt_fix_broken<F: Fn(UpgradeEvent)>(chroot: &mut SystemdNspawn, callback: &F) -> io::Result<()> {
    const ARGS: &[&str] = &["-o", r#"Dpkg::Options::=--force-overwrite"#, "install", "-f", "-y"];

    apt_chroot_command(chroot, ARGS, callback)
}

fn apt_upgrade<F: Fn(UpgradeEvent)>(chroot: &mut SystemdNspawn, callback: &F) -> io::Result<()> {
    const ARGS: &[&str] = &[
        "-o",
        r#"Dpkg::Options::=--force-overwrite"#,
        "full-upgrade",
        "-y",
        "--allow-downgrades",
        "--show-progress",
        "--no-download",
        "--ignore-missing",
    ];

    apt_chroot_command(chroot, ARGS, callback)
}

fn apt_autoremove<F: Fn(UpgradeEvent)>(chroot: &mut SystemdNspawn, callback: &F) -> io::Result<()> {
    const ARGS: &[&str] = &["-o", r#"Dpkg::Options::=--force-overwrite"#, "autoremove", "-y"];

    apt_chroot_command(chroot, ARGS, callback)
}

fn apt_chroot_command<F: Fn(UpgradeEvent)>(
    chroot: &mut SystemdNspawn,
    args: &[&str],
    callback: &F,
) -> io::Result<()> {
    chroot
        .command("apt-get", args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .run_with_callbacks(
            |info| apt_info_callback("apt", info, callback),
            |error| apt_error_callback("apt", error, callback),
        )
}

fn dpkg_configure_all<F: Fn(UpgradeEvent)>(
    chroot: &mut SystemdNspawn,
    callback: &F,
) -> io::Result<()> {
    chroot
        .command("dpkg", &["--configure", "-a"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .run_with_callbacks(
            |info| apt_info_callback("dpkg", info, callback),
            |error| apt_error_callback("dpkg", error, callback),
        )
}

fn attempt<F: Fn(UpgradeEvent)>(chroot: &mut SystemdNspawn, callback: &F) -> io::Result<()> {
    info!("attempting release upgrade");

    if apt_upgrade(chroot, callback).is_err() {
        warn!("release upgrade failed: attempting to repair");
        callback(UpgradeEvent::AttemptingRepair);

        let dpkg_configure = dpkg_configure_all(chroot, callback);
        apt_fix_broken(chroot, callback)?;

        if dpkg_configure.is_err() {
            dpkg_configure_all(chroot, callback)?;
        }

        info!("release upgrade failure repaired: resuming upgrade");
        callback(UpgradeEvent::ResumingUpgrade);
        apt_upgrade(chroot, callback)?;
    }

    callback(UpgradeEvent::Autoremoving);
    apt_autoremove(chroot, callback)?;

    Ok(())
}

fn apt_info_callback<F: Fn(UpgradeEvent)>(cmd: &str, info: &str, callback: &F) {
    if info.is_empty() {
        return;
    }

    info!("{}: info: '{}'", cmd, info);
    let result = info.parse::<AptUpgradeEvent>();
    let event = match result.as_ref() {
        Ok(AptUpgradeEvent::Processing { ref package }) => {
            UpgradeEvent::PackageProcessing(package.as_ref())
        }
        Ok(AptUpgradeEvent::Progress { percent }) => UpgradeEvent::PackageProgress(*percent),
        Ok(AptUpgradeEvent::SettingUp { ref package }) => {
            UpgradeEvent::PackageSettingUp(package.as_ref())
        }
        Ok(AptUpgradeEvent::Unpacking { ref package, ref version, ref over }) => {
            UpgradeEvent::PackageUnpacking {
                package: package.as_ref(),
                version: version.as_ref(),
                over:    over.as_ref(),
            }
        }
        _ => UpgradeEvent::UpgradeInfo(info),
    };

    callback(event);
}

fn apt_error_callback<F: Fn(UpgradeEvent)>(cmd: &str, error: &str, callback: &F) {
    if error.is_empty() {
        return;
    }

    warn!("{}: error: '{}'", cmd, error);
    callback(UpgradeEvent::UpgradeErr(error))
}

fn prepare_mount<C>(disks: &Disks, mut callback: C) -> Result<(), UpgradeError>
where
    C: FnMut(&mut SystemdNspawn) -> Result<(), UpgradeError>,
{
    let _mount_dir = TempDir::new(TARGET).map_err(UpgradeError::ChrootTempCreate)?;

    let _mounts = disks.mount_all_targets(TARGET).map_err(UpgradeError::ChrootMount)?;

    let chroot = &mut SystemdNspawn::new(TARGET).map_err(UpgradeError::ChrootMount)?;
    chroot.env("DEBIAN_FRONTEND", "noninteractive");
    chroot.env("LANG", "C");

    let _efivars_mount = mount_efivars(TARGET).map_err(UpgradeError::EfiVars)?;

    callback(chroot)
}
