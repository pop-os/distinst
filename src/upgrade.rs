use apt_cli_wrappers::AptUpgradeEvent;
use auto::{InstallOption, InstallOptionError, RecoveryOption};
use chroot::SystemdNspawn;
use disks::Disks;
use envfile::EnvFile;
use errors::IoContext;
use external::remount_rw;
use installer::steps::mount_efivars;
use std::io;
use std::process::Stdio;
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
}

#[derive(Debug)]
pub enum UpgradeEvent<'a> {
    AttemptingRepair,
    AttemptingUpgrade,
    DpkgInfo(&'a str),
    DpkgErr(&'a str),
    UpgradeInfo(&'a str),
    UpgradeErr(&'a str),
    Progress(u8),
    ResumingUpgrade,
}

/// Chroot into an existing install, and upgrade it to the next release.
pub fn upgrade<F: Fn(UpgradeEvent), R: Fn() -> bool>(
    disks: &mut Disks,
    option: &RecoveryOption,
    callback: F,
    attempt_repair: R
) -> Result<(), UpgradeError> {
    if !option.upgrade_mode {
        return Err(UpgradeError::ModeNotSet);
    }

    InstallOption::Upgrade(option).apply(disks).map_err(UpgradeError::Configure)?;

    let mount_dir = TempDir::new("/upgrade")
        .map_err(UpgradeError::ChrootTempCreate)?;
    let mount_dir = mount_dir.path();

    let _mounts = disks.mount_all_targets(mount_dir)
        .map_err(UpgradeError::ChrootMount)?;

    let chroot = &mut SystemdNspawn::new(mount_dir).map_err(UpgradeError::ChrootMount)?;
    let _efivars_mount = mount_efivars(mount_dir).map_err(UpgradeError::EfiVars)?;

    fn attempt<F: Fn(UpgradeEvent)>(chroot: &mut SystemdNspawn, callback: &F) -> io::Result<()>{
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

    systemd_boot_entry_restore()?;

    Ok(())
}

fn disable_upgrade_flag() -> io::Result<()> {
    let recovery_conf = &mut EnvFile::new("/cdrom/recovery.conf").with_context(|err| {
        format!("error parsing envfile at /cdrom/recovery.conf: {}", err)
    })?;

    remount_rw("/cdrom")
        .with_context(|err| format!("could not remount /cdrom as rw: {}", err))
        .and_then(|_| {
            recovery_conf.store.remove("UPGRADE");
            recovery_conf.write().with_context(|err| {
                format!("error writing recovery conf: {}", err)
            })
        })
}

fn systemd_boot_entry_restore() -> Result<(), UpgradeError> {
    let mut systemd_boot_conf =
        SystemdBootConf::new("/boot/efi").map_err(UpgradeError::SystemdBootConf)?;

    {
        info!("found the systemd-boot config -- searching for the current entry");
        let SystemdBootConf { ref entries, ref mut loader_conf, .. } = systemd_boot_conf;
        let current_entry = entries
            .iter()
            .find(|e| e.filename.to_lowercase() == "pop_os-current")
            .ok_or(UpgradeError::MissingCurrentEntry)?;

        loader_conf.default = Some(current_entry.filename.to_owned());
    }

    Ok(())
}

fn apt_upgrade<F: Fn(UpgradeEvent)>(chroot: &mut SystemdNspawn, callback: &F) -> io::Result<()> {
    chroot.command("apt-get", &["-y", "--allow-downgrades", "--show-progress", "full-upgrade"])
        .stdout(Stdio::piped())
        .run_with_callbacks(
            |info| {
                info!("apt-info: '{}'", info);
                let event = match info.parse::<AptUpgradeEvent>() {
                    Ok(AptUpgradeEvent::Progress { percent }) => UpgradeEvent::Progress(percent),
                    _ => UpgradeEvent::UpgradeInfo(info)
                };

                callback(event);
            },
            |error| {
                warn!("apt-err: '{}'", error);
                callback(UpgradeEvent::UpgradeErr(error))
            }
        )
}

fn dpkg_configure_all<F: Fn(UpgradeEvent)>(chroot: &mut SystemdNspawn, callback: &F) -> io::Result<()> {
    chroot.command("dpkg", &["--configure", "-a"])
        .run_with_callbacks(
            |info| {
                info!("dpkg-info: '{}'", info);
                callback(UpgradeEvent::DpkgInfo(info));
            },
            |error| {
                warn!("dpkg-err: '{}'", error);
                callback(UpgradeEvent::DpkgErr(error));
            }
        )
}
