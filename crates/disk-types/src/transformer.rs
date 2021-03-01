use crate::{FileSystem, PartitionExt};
use crate::partitioner::Partitioner;
use crate::resizer::PartitionResizer;

use std::path::PathBuf;

pub struct PartitionTransformer<P: Partitioner, R: PartitionResizer> {
    pub parent: PathBuf,
    pub partitioner: P,
    pub resizer: R
}

impl<P: Partitioner, R: PartitionResizer> PartitionTransformer<P, R> {
    pub fn new(partitioner: P, resizer: R, parent: PathBuf) -> Self {
        Self { parent, partitioner, resizer }
    }

    /// Resize the given partition to the given start and end sector.
    pub fn transform<PART: PartitionExt>(
        &self,
        partition: PART,
        new_fs: Option<FileSystem>,
        start: u64,
        end: u64
    ) -> Result<(), ()> {
        Ok(())
    }
}
