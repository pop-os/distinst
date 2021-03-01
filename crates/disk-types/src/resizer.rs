use crate::{FileSystem, Fscker};
use std::io;
use std::path::Path;
use std::process::ExitStatus;

pub trait PartitionResizer {
    fn flush(&self) -> Result<(), ResizeError>;

    fn grow(path: &Path, fs: FileSystem, current: u64, new: u64) -> Result<(), ResizeError>;

    fn shrink(path: &Path, fs: FileSystem, current: u64, new: u64) -> Result<(), ResizeError>;
}

#[derive(Debug, Error)]
pub enum ResizeError {
    #[error(display = "shrinking not supported for {}", fs)]
    ShrinkNotSupported { fs: FileSystem },
    #[error(display = "growing not supported for {}", fs)]
    GrowNotSupported { fs: FileSystem },
    #[error(display = "I/O error occurred while shrinking: {}", _0)]
    Io(io::Error),
    #[error(display = "command failed with exit status: {}", _0)]
    BadStatus(ExitStatus)
}
