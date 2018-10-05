mod accounts;
mod options;
mod retain;

pub(crate) use self::accounts::{AccountFiles, UserData};
pub use self::options::*;
pub(crate) use self::retain::*;
pub use self::retain::delete_old_install;

use super::{DiskError, FileSystemType};
use tempdir::TempDir;
use sys_mount::{Mount, MountFlags, Unmount, UnmountFlags};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Fail)]
pub enum ReinstallError {
    #[fail(display = "no root partition found within the disks configuration")]
    NoRootPartition,
    #[fail(display = "partition {:?} has an invalid file system ({:?})", part, fs)]
    InvalidFilesystem { fs:   FileSystemType, part: PathBuf },
    #[fail(display = "partition could not be mounted: {}", why)]
    PartitionMount { why: io::Error },
    #[fail(display = "error creating temporary directory: {}", why)]
    TempDir { why: io::Error },
    #[fail(display = "I/O error: {}", why)]
    IO { why: io::Error },
    #[fail(display = "no file system found on partition")]
    NoFilesystem,
    #[fail(display = "unable to {} pre-existing account files: {}", step, why)]
    AccountsObtain { why:  io::Error, step: &'static str },
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

impl From<io::Error> for ReinstallError {
    fn from(why: io::Error) -> ReinstallError { ReinstallError::IO { why } }
}

fn mount_and_then<T, F>(
    device: &Path,
    fs: FileSystemType,
    mut action: F,
) -> Result<T, ReinstallError>
where
    F: FnMut(&Path) -> Result<T, ReinstallError>,
{
    let fs: &str = match fs {
        FileSystemType::Fat16 | FileSystemType::Fat32 => {
            return Err(ReinstallError::InvalidFilesystem {
                part: device.to_path_buf(),
                fs,
            });
        }
        fs => fs.into(),
    };

    TempDir::new("distinst")
        .map_err(|why| ReinstallError::TempDir { why })
        .and_then(|tempdir| {
            let base = tempdir.path();
            Mount::new(device, base, fs, MountFlags::empty(), None)
                .map(|m| m.into_unmount_drop(UnmountFlags::DETACH))
                .map_err(|why| ReinstallError::PartitionMount { why })
                .and_then(|_mount| action(base))
        })
}
