mod accounts;
mod options;
mod retain;

pub(crate) use self::accounts::{AccountFiles, UserData};
pub(crate) use self::retain::*;
pub use self::options::*;

use super::{Config, DiskError, Disks, FileSystemType, Installer, Mount};
use tempdir::TempDir;

use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Fail)]
pub enum ReinstallError {
    #[fail(display = "a pre-existing root partition must be specified")]
    NoOldRoot,
    #[fail(display = "no root partition found within the disks configuration")]
    NoRootPartition,
    #[fail(display = "partition {:?} has an invalid file system ({:?})", part, fs)]
    InvalidFilesystem { fs: FileSystemType, part: PathBuf },
    #[fail(display = "partition could not be mounted: {}", why)]
    PartitionMount { why: io::Error },
    #[fail(display = "error creating temporary directory: {}", why)]
    TempDir { why: io::Error },
    #[fail(display = "I/O error: {}", why)]
    IO { why: io::Error },
    #[fail(display = "no file system found on partition")]
    NoFilesystem,
    #[fail(display = "unable to {} pre-existing account files: {}", step, why)]
    AccountsObtain { why: io::Error, step: &'static str },
    #[fail(display = "distinst failed to install: {}", why)]
    Install { why: io::Error },
    #[fail(display = "supplied disk configuration will format /home when it should not")]
    ReformattingHome,
    #[fail(display = "unable to probe existing devices: {}", why)]
    DiskProbe { why: DiskError },
    #[fail(display = "invalid partition configuration: {}", why)]
    InvalidPartitionConfiguration { why: io::Error },
    #[fail(display = "install media at {:?} was not found", path)]
    MissingSquashfs { path: PathBuf },
}

pub fn install_and_retain_home(
    installer: &mut Installer,
    disks: Disks,
    config: &Config
) -> Result<(), ReinstallError> {
    info!("libdistinst: installing while retaining home");
    let account_files;
    let root_path;
    let root_fs;
    let user_data;

    {
        let old_root_uuid = config.old_root.as_ref()
            .ok_or_else(|| ReinstallError::NoOldRoot)?;
        let current_disks = Disks::probe_devices()
            .map_err(|why| ReinstallError::DiskProbe { why })?;
        let old_root = current_disks.get_partition_by_uuid(old_root_uuid)
            .ok_or(ReinstallError::NoRootPartition)?;
        let new_root = disks.get_partition_with_target(Path::new("/"))
            .ok_or(ReinstallError::NoRootPartition)?;

        let (home, home_is_root) = disks.get_partition_with_target(Path::new("/home"))
            .map_or((old_root, true), |p| (p, false));

        if home.will_format() {
            return Err(ReinstallError::ReformattingHome);
        }

        let home_path = home.get_device_path();
        root_path = new_root.get_device_path().to_path_buf();
        root_fs = new_root.filesystem.ok_or_else(|| ReinstallError::NoFilesystem)?;
        let old_root_fs = old_root.filesystem.ok_or_else(|| ReinstallError::NoFilesystem)?;
        let home_fs = home.filesystem.ok_or_else(|| ReinstallError::NoFilesystem)?;

        account_files = AccountFiles::new(old_root.get_device_path(), old_root_fs)?;

        user_data = get_users_on_device(home_path, home_fs, home_is_root)?.iter()
            .filter_map(|user| account_files.get(user))
            .collect::<Vec<_>>();

        // TODO: Back up specific system configurations, such as datetime?

        validate_before_removing(&disks, &config.squashfs, home_path, home_fs)?;
    }

    // Attempt the installation
    installer.install(disks, config)
        .map_err(|why| ReinstallError::Install { why })?;

    // Re-add user data
    add_users_on_device(&root_path, root_fs, &user_data)
}

fn mount_and_then<T, F>(device: &Path, fs: FileSystemType, mut action: F) -> Result<T, ReinstallError>
    where F: FnMut(&Path) -> Result<T, ReinstallError>
{
    let fs = match fs {
        FileSystemType::Fat16 | FileSystemType::Fat32 => {
            return Err(ReinstallError::InvalidFilesystem { part: device.to_path_buf(), fs });
        },
        fs => fs.into(),
    };

    TempDir::new("distinst")
        .map_err(|why| ReinstallError::TempDir { why })
        .and_then(|tempdir| {
            let base = tempdir.path();
            Mount::new(device, base, fs, 0, None)
                .map_err(|why| ReinstallError::PartitionMount { why })
                .and_then(|_mount| action(base))
        })
}
