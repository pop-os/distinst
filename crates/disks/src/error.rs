pub use disk_types::PartitionSizeError;
use disk_types::{FileSystem, PartitionTableError};
use std::{io, path::PathBuf};

/// Defines a variety of errors that may arise from configuring and committing changes to disks.
#[rustfmt::skip]
#[derive(Debug, Fail)]
pub enum DiskError {
    #[fail(display = "decryption error: {}", why)]
    Decryption { why: DecryptionError },
    #[fail(display = "unable to get device at {:?}: {}", device, why)]
    DeviceGet { device: PathBuf, why: io::Error },
    #[fail(display = "unable to probe for devices")]
    DeviceProbe,
    #[fail(display = "unable to commit changes to disk ({:?}): {}", device, why)]
    DiskCommit { device: PathBuf, why: io::Error },
    #[fail(display = "unable to format partition table on {:?}: {}", device, why)]
    DiskFresh { device: PathBuf, why: io::Error },
    #[fail(display = "unable to find disk at {:?}", device)]
    DiskGet { device: PathBuf },
    #[fail(display = "unable to open disk at {:?}: {}", device, why)]
    DiskNew {device: PathBuf,  why: io::Error },
    #[fail(display = "unable to sync disk changes with OS: {}", why)]
    DiskSync { why: io::Error },
    #[fail(display = "unable to encrypt volume '{:?}': {}", volume, why)]
    Encryption { volume: PathBuf, why: io::Error },
    #[fail(display = "unable to open encrypted volume '{:?}': {}", volume, why)]
    EncryptionOpen { volume: PathBuf, why: io::Error },
    #[fail(display = "problem executing external command: {}", why)]
    ExternalCommand { why: io::Error },
    #[fail(display = "serial model does not match")]
    InvalidSerial,
    #[fail(display = "{}", why)]
    IO { why: io::Error },
    #[fail(display = "failed to create partition geometry: {}", why)]
    GeometryCreate { why: io::Error },
    #[fail(display = "failed to duplicate partition geometry")]
    GeometryDuplicate,
    #[fail(display = "failed to set values on partition geometry")]
    GeometrySet,
    #[fail(display = "the root partition may not be contained on a key-encrypted volume")]
    KeyContainsRoot,
    #[fail(display = "LUKS key path was already set for {}", id)]
    KeyPathAlreadySet { id: String },
    #[fail(display = "LUKS keyfile designation lacks key path")]
    KeyWithoutPath,
    #[fail(display = "LUKS keyfile partition does not have a mount target")]
    KeyFileWithoutPath,
    #[fail(display = "partition layout on disk has changed")]
    LayoutChanged,
    #[fail(display = "unable to create logical volume: {}", why)]
    LogicalVolumeCreate { why: io::Error },
    #[fail(display = "logical partition '{}-{}' does not exist", group, volume)]
    LogicalPartitionNotFound { group: String, volume: String },
    #[fail(display = "unable to get mount points: {}", why)]
    MountsObtain { why: io::Error },
    #[fail(display = "new partition could not be found")]
    NewPartNotFound,
    #[fail(display = "partition error ({:?}): {}", partition, why)]
    PartitionError { partition: PathBuf, why: PartitionError },
    #[fail(display = "partition {} not be found on disk", partition)]
    PartitionNotFound { partition: i32 },
    #[fail(display = "partition exceeds size of disk")]
    PartitionOOB,
    #[fail(display = "unable to remove partition {}: {}", partition, why)]
    PartitionRemove { partition: i32, why: io::Error },
    #[fail(display = "unable to remove partition at sector {}: {}", sector, why)]
    PartitionRemoveBySector { sector: u64, why: io::Error },
    #[fail(display = "{}", why)]
    PartitionTable { why: PartitionTableError },
    #[fail(display = "unable to create physical volume from '{}': {}", volume, why)]
    PhysicalVolumeCreate { volume: String, why: io::Error },
    #[fail(display = "multiple devices had the same volume group: currently unsupported")]
    SameGroup,
    #[fail(display = "sector overlaps partition {}", id)]
    SectorOverlaps { id: i32 },
    #[fail(display = "unable to get serial model of device: {}", why)]
    SerialGet { why: io::Error },
    #[fail(display = "unable to unmount partition(s) on {:?}: {}", device, why)]
    Unmount { device: PathBuf, why: io::Error },
    #[fail(display = "unable to create volume group '{}' on {:?}: {}", vg, device, why)]
    VolumeGroupCreate { device: PathBuf, vg: String, why: io::Error },
    #[fail(display = "logical partition on {:?} lacks a label", device)]
    VolumePartitionLacksLabel { device: PathBuf },
}

#[derive(Debug, Fail)]
/// An error that involves partitions.
pub enum PartitionError {
    #[fail(display = "no file system was found on the partition")]
    NoFilesystem,
    #[fail(display = "unable to format partition: {}", why)]
    PartitionFormat { why: io::Error },
    #[fail(display = "partition overlaps other partitions")]
    PartitionOverlaps,
    #[fail(display = "unable to move partition: {}", why)]
    PartitionMove { why: io::Error },
    #[fail(display = "unable to resize partition: {}", why)]
    PartitionResize { why: io::Error },
    #[fail(display = "partition was too large (size: {}, max: {}", size, max)]
    PartitionTooLarge { size: u64, max: u64 },
    #[fail(display = "partition was too small (size: {}, min: {})", size, min)]
    PartitionTooSmall { size: u64, min: u64 },
    #[fail(display = "unable to create partition: {}", why)]
    PartitionCreate { why: io::Error },
    #[fail(display = "partition resize value is too small")]
    ResizeTooSmall,
    #[fail(display = "shrink value too high")]
    ShrinkValueTooHigh,
    #[fail(display = "shrinking not supported for {:?}", fs)]
    UnsupportedShrinking { fs: FileSystem },
}

#[derive(Debug, Fail)]
pub enum DecryptionError {
    #[fail(display = "failed to decrypt '{:?}': {}", device, why)]
    Open { device: PathBuf, why: io::Error },
    #[fail(display = "decrypted partition, '{:?}', lacks volume group", device)]
    DecryptedLacksVG { device: PathBuf },
    #[fail(display = "LUKS partition at '{:?}' was not found", device)]
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
