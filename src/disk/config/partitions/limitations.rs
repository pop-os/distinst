use super::super::super::PartitionSizeError;
use super::FileSystemType;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = MIB * 1024;
const TIB: u64 = GIB * 1024;

const FAT16_MIN: u64 = 16 * MIB;
const FAT16_MAX: u64 = (4096 - 1) * MIB;
const FAT32_MIN: u64 = 33 * MIB;
const FAT32_MAX: u64 = 2 * TIB;
const EXT4_MAX: u64 = 16 * TIB;
const BTRFS_MIN: u64 = 250 * MIB;

/// Determines if the supplied file system size is valid for the given file
/// system type.
pub fn check_partition_size(size: u64, fs: FileSystemType) -> Result<(), PartitionSizeError> {
    match fs {
        FileSystemType::Btrfs if size < BTRFS_MIN => {
            Err(PartitionSizeError::TooSmall(size, BTRFS_MIN))
        }
        FileSystemType::Fat16 if size < FAT16_MIN => {
            Err(PartitionSizeError::TooSmall(size, FAT16_MIN))
        }
        FileSystemType::Fat16 if size > FAT16_MAX => {
            Err(PartitionSizeError::TooLarge(size, FAT16_MAX))
        }
        FileSystemType::Fat32 if size < FAT32_MIN => {
            Err(PartitionSizeError::TooSmall(size, FAT32_MIN))
        }
        FileSystemType::Fat32 if size > FAT32_MAX => {
            Err(PartitionSizeError::TooLarge(size, FAT32_MAX))
        }
        FileSystemType::Ext4 if size > EXT4_MAX => {
            Err(PartitionSizeError::TooLarge(size, EXT4_MAX))
        }
        _ => Ok(()),
    }
}
