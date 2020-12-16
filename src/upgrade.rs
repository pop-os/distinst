use apt_cli_wrappers::AptUpgradeEvent;
use crate::auto::{InstallOption, InstallOptionError, RecoveryOption};
use crate::chroot::SystemdNspawn;
use err_derive::Error;
use crate::disks::Disks;
use crate::errors::IoContext;
use crate::external::remount_rw;
use crate::installer::{steps::mount_efivars, RecoveryEnv};
use std::{io, path::Path, process::Stdio};
use systemd_boot_conf::SystemdBootConf;
use tempdir::TempDir;

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
    #[error(display = "failed to remove upgrade flag from recovery.conf: {}", _0)]
    UpgradeFlag(io::Error),
}

#[derive(Debug)]
pub enum UpgradeEvent<'a> {
    AttemptingRepair,
    AttemptingUpgrade,
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
pub fn upgrade<F: Fn(UpgradeEvent), R: Fn() -> bool>(
    recovery_conf: &mut RecoveryEnv,
    disks: &mut Disks,
    option: &RecoveryOption,
    callback: F,
    attempt_repair: R,
) -> Result<(), UpgradeError> {
    if option.mode.as_deref() != Some("upgrade") {
        return Err(UpgradeError::ModeNotSet);
    }

    InstallOption::Upgrade(option).apply(disks).map_err(UpgradeError::Configure)?;

    let mount_dir = TempDir::new("/upgrade").map_err(UpgradeError::ChrootTempCreate)?;
    let mount_dir = mount_dir.path();

    let _mounts = disks.mount_all_targets(mount_dir).map_err(UpgradeError::ChrootMount)?;

    let chroot = &mut SystemdNspawn::new(mount_dir).map_err(UpgradeError::ChrootMount)?;
    chroot.env("DEBIAN_FRONTEND", "noninteractive");
    chroot.env("LANG", "C");

    let _efivars_mount = mount_efivars(mount_dir).map_err(UpgradeError::EfiVars)?;

    fn attempt<F: Fn(UpgradeEvent)>(chroot: &mut SystemdNspawn, callback: &F) -> io::Result<()> {
        info!("attempting release upgrade");
        if apt_upgrade(chroot, callback).is_err() {
            warn!("release upgrade failed: attempting to repair");
            callback(UpgradeEvent::AttemptingRepair);
            dpkg_configure_all(chroot, callback)?;

            info!("release upgrade failure repaired: resuming upgrade");
            callback(UpgradeEvent::ResumingUpgrade);
            apt_upgrade(chroot, callback)?;
        }

        Ok(())
    }

    callback(UpgradeEvent::AttemptingUpgrade);
    if let Err(why) = attempt(chroot, &callback) {
        error!("upgrade attempt failed: {}", why);
        if !attempt_repair() {
            return Err(UpgradeError::AttemptFailed(why));
        }

        if let Err(why) = attempt(chroot, &callback) {
            return Err(UpgradeError::AttemptFailed(why));
        }
    }

    remount_rw("/cdrom")
        .with_context(|err| format!("could not remount /cdrom as rw: {}", err))
        .and_then(|_| {
            recovery_conf.remove("MODE");
            recovery_conf.write()
        })
        .map_err(UpgradeError::UpgradeFlag)?;

    systemd_boot_entry_restore(mount_dir)?;

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
            .find(|e| e.id.to_lowercase() == "pop_os-current")
            .ok_or(UpgradeError::MissingCurrentEntry)?;

        loader_conf.default = Some(current_entry.id.to_owned());
    }

    Ok(())
}

fn apt_upgrade<F: Fn(UpgradeEvent)>(chroot: &mut SystemdNspawn, callback: &F) -> io::Result<()> {
    chroot
        .command("apt-get", &["-y", "--allow-downgrades", "--show-progress", "full-upgrade"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .run_with_callbacks(
            |info| {
                info!("apt-info: '{}'", info);
                let result = info.parse::<AptUpgradeEvent>();
                let event = match result.as_ref() {
                    Ok(AptUpgradeEvent::Processing { ref package }) => {
                        UpgradeEvent::PackageProcessing(package.as_ref())
                    }
                    Ok(AptUpgradeEvent::Progress { percent }) => {
                        UpgradeEvent::PackageProgress(*percent)
                    }
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
            },
            |error| {
                warn!("apt-err: '{}'", error);
                callback(UpgradeEvent::UpgradeErr(error))
            },
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
            |info| {
                info!("dpkg-info: '{}'", info);
                let result = info.parse::<AptUpgradeEvent>();
                let event = match result.as_ref() {
                    Ok(AptUpgradeEvent::Processing { ref package }) => {
                        UpgradeEvent::PackageProcessing(package.as_ref())
                    }
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
            },
            |error| {
                warn!("dpkg-err: '{}'", error);
                callback(UpgradeEvent::DpkgErr(error));
            },
        )
}
