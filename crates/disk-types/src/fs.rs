use std::{fmt, str::FromStr};
use sys_mount::FilesystemType as MountFS;

/// Describes a file system format, such as ext4 or fat32.
#[derive(Debug, PartialEq, Copy, Clone, Hash)]
pub enum FileSystem {
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

/// Indicates that a partition is either too small or too large.
#[derive(Debug)]
pub enum PartitionSizeError {
    TooSmall(u64, u64),
    TooLarge(u64, u64),
}

impl FileSystem {
    /// Check if a given size, in bytes, is valid for this file system.
    ///
    /// # Possible Values
    /// - `Ok(())` indicates a valid partition size.
    /// - `Err(PartitionSizeError::TooSmall)` for a partition that is too small.
    /// - `Err(PartitionSizeError::TooLarge)` for a partition that is too large.
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
            FileSystem::Btrfs if size < BTRFS_MIN => {
                Err(PartitionSizeError::TooSmall(size, BTRFS_MIN))
            }
            FileSystem::Fat16 if size < FAT16_MIN => {
                Err(PartitionSizeError::TooSmall(size, FAT16_MIN))
            }
            FileSystem::Fat16 if size > FAT16_MAX => {
                Err(PartitionSizeError::TooLarge(size, FAT16_MAX))
            }
            FileSystem::Fat32 if size < FAT32_MIN => {
                Err(PartitionSizeError::TooSmall(size, FAT32_MIN))
            }
            FileSystem::Fat32 if size > FAT32_MAX => {
                Err(PartitionSizeError::TooLarge(size, FAT32_MAX))
            }
            FileSystem::Ext4 if size > EXT4_MAX => {
                Err(PartitionSizeError::TooLarge(size, EXT4_MAX))
            }
            _ => Ok(()),
        }
    }
}

impl FromStr for FileSystem {
    type Err = &'static str;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        let type_ = match string.to_lowercase().as_str() {
            "btrfs" => FileSystem::Btrfs,
            "exfat" => FileSystem::Exfat,
            "ext2" => FileSystem::Ext2,
            "ext3" => FileSystem::Ext3,
            "ext4" => FileSystem::Ext4,
            "f2fs" => FileSystem::F2fs,
            "fat16" => FileSystem::Fat16,
            "fat32" => FileSystem::Fat32,
            "swap" | "linux-swap(v1)" => FileSystem::Swap,
            "ntfs" => FileSystem::Ntfs,
            "xfs" => FileSystem::Xfs,
            "lvm" | "lvm2_member" => FileSystem::Lvm,
            "luks" | "crypto_luks" => FileSystem::Luks,
            _ => return Err("invalid file system name"),
        };
        Ok(type_)
    }
}

impl From<FileSystem> for &'static str {
    fn from(val: FileSystem) -> Self {
        match val {
            FileSystem::Btrfs => "btrfs",
            FileSystem::Exfat => "exfat",
            FileSystem::Ext2 => "ext2",
            FileSystem::Ext3 => "ext3",
            FileSystem::Ext4 => "ext4",
            FileSystem::F2fs => "f2fs",
            FileSystem::Fat16 => "fat16",
            FileSystem::Fat32 => "fat32",
            FileSystem::Ntfs => "ntfs",
            FileSystem::Swap => "linux-swap(v1)",
            FileSystem::Xfs => "xfs",
            FileSystem::Lvm => "lvm",
            FileSystem::Luks => "luks",
        }
    }
}

impl fmt::Display for FileSystem {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let str: &'static str = (*self).into();
        f.write_str(str)
    }
}

/// Enable integration with the `sys_mount` crate, if it is used.
impl From<FileSystem> for MountFS<'static> {
    fn from(fs: FileSystem) -> Self {
        MountFS::Manual(match fs {
            FileSystem::Fat16 | FileSystem::Fat32 => "vfat",
            fs => fs.into(),
        })
    }
}
