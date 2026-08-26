pub use disk_types::PartitionSizeError;
use disk_types::{FileSystem, PartitionTableError};
use std::{io, path::PathBuf};

/// Defines a variety of errors that may arise from configuring and committing changes to disks.
#[rustfmt::skip]
#[derive(Debug, thiserror::Error)]
pub enum DiskError {
    #[error("decryption error: {}", why)]
    Decryption { why: DecryptionError },
    #[error("unable to get device at {:?}: {}", device, why)]
    DeviceGet { device: PathBuf, why: io::Error },
    #[error("unable to probe for devices")]
    DeviceProbe,
    #[error("unable to commit changes to disk ({:?}): {}", device, why)]
    DiskCommit { device: PathBuf, why: io::Error },
    #[error("unable to format partition table on {:?}: {}", device, why)]
    DiskFresh { device: PathBuf, why: io::Error },
    #[error("unable to find disk at {:?}", device)]
    DiskGet { device: PathBuf },
    #[error("unable to open disk at {:?}: {}", device, why)]
    DiskNew {device: PathBuf,  why: io::Error },
    #[error("unable to sync disk changes with OS: {}", why)]
    DiskSync { why: io::Error },
    #[error("unable to encrypt volume '{:?}': {}", volume, why)]
    Encryption { volume: PathBuf, why: io::Error },
    #[error("unable to open encrypted volume '{:?}': {}", volume, why)]
    EncryptionOpen { volume: PathBuf, why: io::Error },
    #[error("problem executing external command: {}", why)]
    ExternalCommand { why: io::Error },
    #[error("serial model does not match")]
    InvalidSerial,
    #[error("{}", why)]
    IO { why: io::Error },
    #[error("failed to create partition geometry: {}", why)]
    GeometryCreate { why: io::Error },
    #[error("failed to duplicate partition geometry")]
    GeometryDuplicate,
    #[error("failed to set values on partition geometry")]
    GeometrySet,
    #[error("the root partition may not be contained on a key-encrypted volume")]
    KeyContainsRoot,
    #[error("LUKS key path was already set for {}", id)]
    KeyPathAlreadySet { id: String },
    #[error("LUKS keyfile designation lacks key path")]
    KeyWithoutPath,
    #[error("LUKS keyfile partition does not have a mount target")]
    KeyFileWithoutPath,
    #[error("partition layout on disk has changed")]
    LayoutChanged,
    #[error("unable to create logical volume: {}", why)]
    LogicalVolumeCreate { why: io::Error },
    #[error("logical partition '{}-{}' does not exist", group, volume)]
    LogicalPartitionNotFound { group: String, volume: String },
    #[error("unable to get mount points: {}", why)]
    MountsObtain { why: io::Error },
    #[error("new partition could not be found")]
    NewPartNotFound,
    #[error("partition error ({:?}): {}", partition, why)]
    PartitionError { partition: PathBuf, why: PartitionError },
    #[error("partition {} not be found on disk", partition)]
    PartitionNotFound { partition: i32 },
    #[error("partition exceeds size of disk")]
    PartitionOOB,
    #[error("unable to remove partition {}: {}", partition, why)]
    PartitionRemove { partition: i32, why: io::Error },
    #[error("unable to remove partition at sector {}: {}", sector, why)]
    PartitionRemoveBySector { sector: u64, why: io::Error },
    #[error("{}", why)]
    PartitionTable { why: PartitionTableError },
    #[error("unable to create physical volume from '{}': {}", volume, why)]
    PhysicalVolumeCreate { volume: String, why: io::Error },
    #[error("multiple devices had the same volume group: currently unsupported")]
    SameGroup,
    #[error("sector overlaps partition {}", id)]
    SectorOverlaps { id: i32 },
    #[error("unable to get serial model of device: {}", why)]
    SerialGet { why: io::Error },
    #[error("unable to unmount partition(s) on {:?}: {}", device, why)]
    Unmount { device: PathBuf, why: io::Error },
    #[error("unable to create volume group '{}' on {:?}: {}", vg, device, why)]
    VolumeGroupCreate { device: PathBuf, vg: String, why: io::Error },
    #[error("logical partition on {:?} lacks a label", device)]
    VolumePartitionLacksLabel { device: PathBuf },
}

#[derive(Debug, thiserror::Error)]
/// An error that involves partitions.
pub enum PartitionError {
    #[error("no file system was found on the partition")]
    NoFilesystem,
    #[error("unable to format partition: {}", why)]
    PartitionFormat { why: io::Error },
    #[error("partition overlaps other partitions")]
    PartitionOverlaps,
    #[error("unable to move partition: {}", why)]
    PartitionMove { why: io::Error },
    #[error("unable to resize partition: {}", why)]
    PartitionResize { why: io::Error },
    #[error("partition was too large (size: {}, max: {}", size, max)]
    PartitionTooLarge { size: u64, max: u64 },
    #[error("partition was too small (size: {}, min: {})", size, min)]
    PartitionTooSmall { size: u64, min: u64 },
    #[error("unable to create partition: {}", why)]
    PartitionCreate { why: io::Error },
    #[error("partition resize value is too small")]
    ResizeTooSmall,
    #[error("shrink value too high")]
    ShrinkValueTooHigh,
    #[error("shrinking not supported for {:?}", fs)]
    UnsupportedShrinking { fs: FileSystem },
}

#[derive(Debug, thiserror::Error)]
pub enum DecryptionError {
    #[error("failed to decrypt '{:?}': {}", device, why)]
    Open { device: PathBuf, why: io::Error },
    #[error("decrypted partition, '{:?}', lacks volume group", device)]
    DecryptedLacksVG { device: PathBuf },
    #[error("LUKS partition at '{:?}' was not found", device)]
    LuksNotFound { device: PathBuf },
}

impl From<DecryptionError> for DiskError {
    fn from(why: DecryptionError) -> DiskError { DiskError::Decryption { why } }
}

impl DiskError {
    pub fn new_partition_error<E: Into<PartitionError>>(partition: PathBuf, why: E) -> DiskError {
        DiskError::PartitionError { partition, why: why.into() }
    }
}

impl From<io::Error> for DiskError {
    fn from(why: io::Error) -> DiskError { DiskError::IO { why } }
}

impl From<DiskError> for io::Error {
    fn from(err: DiskError) -> io::Error {
        io::Error::new(io::ErrorKind::Other, format!("an I/O error occurred: {}", err))
    }
}

impl From<PartitionSizeError> for PartitionError {
    fn from(err: PartitionSizeError) -> PartitionError {
        match err {
            PartitionSizeError::TooSmall(size, min) => {
                PartitionError::PartitionTooSmall { size, min }
            }
            PartitionSizeError::TooLarge(size, max) => {
                PartitionError::PartitionTooLarge { size, max }
            }
        }
    }
}

impl From<PartitionTableError> for DiskError {
    fn from(why: PartitionTableError) -> DiskError { DiskError::PartitionTable { why } }
}
