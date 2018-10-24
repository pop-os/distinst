extern crate sys_mount;

use std::str::FromStr;
use sys_mount::FilesystemType as MountFS;

/// Describes a file system format, such as ext4 or fat32.
#[derive(Debug, PartialEq, Copy, Clone, Hash)]
pub enum FileSystemType {
    Btrfs,
    Exfat,
    Ext2,
    Ext3,
    Ext4,
    F2fs,
    Fat16,
    Fat32,
    Ntfs,
    Swap,
    Xfs,
    Luks,
    Lvm,
}

pub enum PartitionSizeError {
    TooSmall(u64, u64),
    TooLarge(u64, u64),
}

impl FileSystemType {
    pub fn validate_size(self, size: u64) -> Result<(), PartitionSizeError> {
        const MIB: u64 = 1024 * 1024;
        const GIB: u64 = MIB * 1024;
        const TIB: u64 = GIB * 1024;

        const FAT16_MIN: u64 = 16 * MIB;
        const FAT16_MAX: u64 = (4096 - 1) * MIB;
        const FAT32_MIN: u64 = 33 * MIB;
        const FAT32_MAX: u64 = 2 * TIB;
        const EXT4_MAX: u64 = 16 * TIB;
        const BTRFS_MIN: u64 = 250 * MIB;

        match self {
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
}

impl FromStr for FileSystemType {
    type Err = &'static str;
    fn from_str(string: &str) -> Result<Self, Self::Err> {
        let type_ = match string {
            "btrfs" => FileSystemType::Btrfs,
            "exfat" => FileSystemType::Exfat,
            "ext2" => FileSystemType::Ext2,
            "ext3" => FileSystemType::Ext3,
            "ext4" => FileSystemType::Ext4,
            "f2fs" => FileSystemType::F2fs,
            "fat16" => FileSystemType::Fat16,
            "fat32" => FileSystemType::Fat32,
            "swap" | "linux-swap(v1)" => FileSystemType::Swap,
            "ntfs" => FileSystemType::Ntfs,
            "xfs" => FileSystemType::Xfs,
            "lvm" => FileSystemType::Lvm,
            "luks" => FileSystemType::Luks,
            _ => return Err("invalid file system name"),
        };
        Ok(type_)
    }
}

impl Into<&'static str> for FileSystemType {
    fn into(self) -> &'static str {
        match self {
            FileSystemType::Btrfs => "btrfs",
            FileSystemType::Exfat => "exfat",
            FileSystemType::Ext2 => "ext2",
            FileSystemType::Ext3 => "ext3",
            FileSystemType::Ext4 => "ext4",
            FileSystemType::F2fs => "f2fs",
            FileSystemType::Fat16 => "fat16",
            FileSystemType::Fat32 => "fat32",
            FileSystemType::Ntfs => "ntfs",
            FileSystemType::Swap => "linux-swap(v1)",
            FileSystemType::Xfs => "xfs",
            FileSystemType::Lvm => "lvm",
            FileSystemType::Luks => "luks",
        }
    }
}

/// Enable integration with the `sys_mount` crate, if it is used.
impl From<FileSystemType> for MountFS<'static> {
    fn from(fs: FileSystemType) -> Self {
        MountFS::Manual(match fs {
            FileSystemType::Fat16 | FileSystemType::Fat32 => "vfat",
            fs => fs.into(),
        })
    }
}

/// Defines whether the partition is a primary, logical, or extended partition.
///
/// # Note
///
/// This only applies for MBR partition tables.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionType {
    Primary,
    Logical,
    Extended,
}
