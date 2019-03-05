use apt_cli_wrappers::AptUpgradeEvent;
use auto::{InstallOption, InstallOptionError, RecoveryOption};
use chroot::Chroot;
use disks::Disks;
use installer::steps::mount_efivars;
use std::io;
use tempdir::TempDir;
use std::process::Stdio;

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
    #[error(display = "attempted an upgrade, but the upgrade mode was not set")]
    ModeNotSet,
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

    let chroot = &mut Chroot::new(mount_dir).map_err(UpgradeError::ChrootMount)?;
    let _efivars_mount = mount_efivars(mount_dir).map_err(UpgradeError::EfiVars)?;

    fn attempt<F: Fn(UpgradeEvent)>(chroot: &mut Chroot, callback: &F) -> io::Result<()>{
        if apt_upgrade(chroot, callback).is_err() {
            callback(UpgradeEvent::AttemptingRepair);
            dpkg_configure_all(chroot, callback)?;
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
    }

    Ok(())
}

fn apt_upgrade<F: Fn(UpgradeEvent)>(chroot: &mut Chroot, callback: &F) -> io::Result<()> {
    chroot.command("apt-get", &["-y", "--allow-downgrades", "--show-progress", "full-upgrade"])
        .stdout(Stdio::piped())
        .run_with_callbacks(
            |info| {
                let event = match info.parse::<AptUpgradeEvent>() {
                    Ok(AptUpgradeEvent::Progress { percent }) => UpgradeEvent::Progress(percent),
                    _ => UpgradeEvent::UpgradeInfo(info)
                };

                callback(event);
            },
            |error| callback(UpgradeEvent::UpgradeErr(error))
        )
}

fn dpkg_configure_all<F: Fn(UpgradeEvent)>(chroot: &mut Chroot, callback: &F) -> io::Result<()> {
    chroot.command("dpkg", &["--configure", "-a"]).run_with_callbacks(
        |info| callback(UpgradeEvent::DpkgInfo(info)),
        |error| callback(UpgradeEvent::DpkgErr(error))
    )
}
