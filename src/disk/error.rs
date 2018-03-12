use std::io;
use std::path::PathBuf;

/// Defines a variety of errors that may arise from configuring and committing changes to disks.
#[cfg_attr(rustfmt, rustfmt_skip)]
#[derive(Debug, Fail)]
pub enum DiskError {
    #[fail(display = "failed to decrypt '{:?}': {}", device, why)]
    Decryption { device: PathBuf, why: io::Error },
    #[fail(display = "decrypted partition, '{:?}', lacks volume group", device)]
    DecryptedLacksVG { device: PathBuf },
    #[fail(display = "unable to get device: {}", why)]
    DeviceGet { why: io::Error },
    #[fail(display = "unable to probe for devices")]
    DeviceProbe,
    #[fail(display = "unable to commit changes to disk: {}", why)]
    DiskCommit { why: io::Error },
    #[fail(display = "unable to format partition table: {}", why)]
    DiskFresh { why: io::Error },
    #[fail(display = "unable to find disk")]
    DiskGet,
    #[fail(display = "unable to open disk: {}", why)]
    DiskNew { why: io::Error },
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
    #[fail(display = "LUKS partition at '{:?}' was not found", device)]
    LuksNotFound { device: PathBuf },
    #[fail(display = "unable to get mount points: {}", why)]
    MountsObtain { why: io::Error },
    #[fail(display = "new partition could not be found")]
    NewPartNotFound,
    #[fail(display = "no file system was found on the partition")]
    NoFilesystem,
    #[fail(display = "unable to create partition: {}", why)]
    PartitionCreate { why: io::Error },
    #[fail(display = "unable to format partition: {}", why)]
    PartitionFormat { why: io::Error },
    #[fail(display = "partition {} not be found on disk", partition)]
    PartitionNotFound { partition: i32 },
    #[fail(display = "partition overlaps other partitions")]
    PartitionOverlaps,
    #[fail(display = "unable to remove partition {}: {}", partition, why)]
    PartitionRemove { partition: i32, why: io::Error },
    #[fail(display = "unable to move partition: {}", why)]
    PartitionMove { why: io::Error },
    #[fail(display = "unable to resize partition: {}", why)]
    PartitionResize { why: io::Error },
    #[fail(display = "partition table not found on disk")]
    PartitionTableNotFound,
    #[fail(display = "partition was too large (size: {}, max: {}", size, max)]
    PartitionTooLarge { size: u64, max:  u64 },
    #[fail(display = "partition was too small (size: {}, min: {})", size, min)]
    PartitionTooSmall { size: u64, min:  u64 },
    #[fail(display = "partition exceeds size of disk")]
    PartitionOOB,
    #[fail(display = "unable to create physical volume from '{}': {}", volume, why)]
    PhysicalVolumeCreate { volume: String, why: io::Error },
    #[fail(display = "too many primary partitions in MSDOS partition table")]
    PrimaryPartitionsExceeded,
    #[fail(display = "multiple devices had the same volume group: currently unsupported")]
    SameGroup,
    #[fail(display = "sector overlaps partition {}", id)]
    SectorOverlaps { id: i32 },
    #[fail(display = "unable to get serial model of device: {}", why)]
    SerialGet { why: io::Error },
    #[fail(display = "partition resize value is too small")]
    ResizeTooSmall,
    #[fail(display = "unable to unmount partition(s): {}", why)]
    Unmount { why: io::Error },
    #[fail(display = "shrinking not supported for this file system")]
    UnsupportedShrinking,
    #[fail(display = "volume activation failed: {}", why)]
    VolumeActivation { why: io::Error },
    #[fail(display = "unable to create volume group: {}", why)]
    VolumeGroupCreate { why: io::Error },
    #[fail(display = "volume partition lacks a label")]
    VolumePartitionLacksLabel,
}

impl From<DiskError> for io::Error {
    fn from(err: DiskError) -> io::Error {
        io::Error::new(io::ErrorKind::Other, format!("{}", err))
    }
}

pub enum PartitionSizeError {
    TooSmall(u64, u64),
    TooLarge(u64, u64),
}

impl From<PartitionSizeError> for DiskError {
    fn from(err: PartitionSizeError) -> DiskError {
        match err {
            PartitionSizeError::TooSmall(size, min) => DiskError::PartitionTooSmall { size, min },
            PartitionSizeError::TooLarge(size, max) => DiskError::PartitionTooLarge { size, max },
        }
    }
}
