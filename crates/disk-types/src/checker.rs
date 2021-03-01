use crate::fs::FileSystem;
use std::io;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

pub trait Fscker {
    fn fsck(path: &Path, fs: FileSystem) -> Result<(), FsckError>;
}

#[derive(Debug, Error)]
pub enum FsckError {
    #[error(display = "fsck I/O error: {:?}", _0)]
    Io(io::Error),
    #[error(display = "command failed with exit status: {}", _0)]
    BadStatus(ExitStatus),
}
