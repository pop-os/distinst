//! Automatic installation options, and actions executed by them.

mod accounts;
mod options;
mod retain;

pub(crate) use self::{
    accounts::{AccountFiles, UserData},
    retain::*,
};
pub use self::{options::*, retain::delete_old_install};

use disk_types::FileSystem;
use std::{
    io,
    path::PathBuf,
};

#[derive(Debug, Fail)]
pub enum ReinstallError {
    #[fail(display = "no root partition found within the disks configuration")]
    NoRootPartition,
    #[fail(display = "partition {:?} has an invalid file system ({:?})", part, fs)]
    InvalidFilesystem { fs: FileSystem, part: PathBuf },
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
    DiskProbe { why: crate::disks::DiskError },
    #[fail(display = "invalid partition configuration: {}", why)]
    InvalidPartitionConfiguration { why: io::Error },
    #[fail(display = "install media at {:?} was not found", path)]
    MissingSquashfs { path: PathBuf },
}

impl From<io::Error> for ReinstallError {
    fn from(why: io::Error) -> ReinstallError { ReinstallError::IO { why } }
}
