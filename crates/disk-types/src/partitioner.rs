use crate::{FileSystem, PartitionType};
use libparted::PartitionFlag;
use std::io;
use std::path::{Path, PathBuf};

pub struct NewPartition {
    pub start: u64,
    pub end: u64,
    pub fs: Option<FileSystem>,
    pub label: Option<String>,
    pub flags: Vec<PartitionFlag>,
    pub kind: PartitionType,
}

#[derive(Debug, Error)]
pub enum PartitionError {
    #[error(display = "failed to open disk: {}", _0)]
    OpenDisk(io::Error),
    #[error(display = "failed to remove partition: {}", _0)]
    RemovePartition(io::Error),
    #[error(display = "failed to commit to disk: {}", _0)]
    CommitToDisk(io::Error),
    #[error(display = "failed to create partition: {}", _0)]
    CreatePartition(io::Error),
    #[error(display = "failed to retrieve new partition info: {}", _0)]
    GetNewData(io::Error)

}

pub trait Partitioner {
    fn create(&mut self, device: &Path, data: NewPartition) -> Result<(), PartitionError>;

    fn delete(&mut self, device: &Path, number: u32) -> Result<Result<(Option<u32>, PathBuf), PartitionError>, PartitionError>;
}
