use libc;

use std::ffi::CStr;

use distinst::FileSystem;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DISTINST_FILE_SYSTEM {
    NONE = 0,
    BTRFS = 1,
    EXFAT = 2,
    EXT2 = 3,
    EXT3 = 4,
    EXT4 = 5,
    F2FS = 6,
    FAT16 = 7,
    FAT32 = 8,
    NTFS = 9,
    SWAP = 10,
    XFS = 11,
    LVM = 12,
    LUKS = 13,
}

impl From<DISTINST_FILE_SYSTEM> for Option<FileSystem> {
    fn from(fs: DISTINST_FILE_SYSTEM) -> Option<FileSystem> {
        match fs {
            DISTINST_FILE_SYSTEM::BTRFS => Some(FileSystem::Btrfs),
            DISTINST_FILE_SYSTEM::EXFAT => Some(FileSystem::Exfat),
            DISTINST_FILE_SYSTEM::EXT2 => Some(FileSystem::Ext2),
            DISTINST_FILE_SYSTEM::EXT3 => Some(FileSystem::Ext3),
            DISTINST_FILE_SYSTEM::EXT4 => Some(FileSystem::Ext4),
            DISTINST_FILE_SYSTEM::F2FS => Some(FileSystem::F2fs),
            DISTINST_FILE_SYSTEM::FAT16 => Some(FileSystem::Fat16),
            DISTINST_FILE_SYSTEM::FAT32 => Some(FileSystem::Fat32),
            DISTINST_FILE_SYSTEM::NONE => None,
            DISTINST_FILE_SYSTEM::NTFS => Some(FileSystem::Ntfs),
            DISTINST_FILE_SYSTEM::SWAP => Some(FileSystem::Swap),
            DISTINST_FILE_SYSTEM::XFS => Some(FileSystem::Xfs),
            DISTINST_FILE_SYSTEM::LVM => Some(FileSystem::Lvm),
            DISTINST_FILE_SYSTEM::LUKS => Some(FileSystem::Luks),
        }
    }
}

impl From<FileSystem> for DISTINST_FILE_SYSTEM {
    fn from(fs: FileSystem) -> DISTINST_FILE_SYSTEM {
        match fs {
            FileSystem::Btrfs => DISTINST_FILE_SYSTEM::BTRFS,
            FileSystem::Exfat => DISTINST_FILE_SYSTEM::EXFAT,
            FileSystem::Ext2 => DISTINST_FILE_SYSTEM::EXT2,
            FileSystem::Ext3 => DISTINST_FILE_SYSTEM::EXT3,
            FileSystem::Ext4 => DISTINST_FILE_SYSTEM::EXT4,
            FileSystem::F2fs => DISTINST_FILE_SYSTEM::F2FS,
            FileSystem::Fat16 => DISTINST_FILE_SYSTEM::FAT16,
            FileSystem::Fat32 => DISTINST_FILE_SYSTEM::FAT32,
            FileSystem::Ntfs => DISTINST_FILE_SYSTEM::NTFS,
            FileSystem::Swap => DISTINST_FILE_SYSTEM::SWAP,
            FileSystem::Xfs => DISTINST_FILE_SYSTEM::XFS,
            FileSystem::Lvm => DISTINST_FILE_SYSTEM::LVM,
            FileSystem::Luks => DISTINST_FILE_SYSTEM::LUKS,
        }
    }
}

impl DISTINST_FILE_SYSTEM {
    fn get_cstr(self) -> *const libc::c_char {
        match self {
            DISTINST_FILE_SYSTEM::BTRFS => CStr::from_bytes_with_nul(b"btrfs\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::EXFAT => CStr::from_bytes_with_nul(b"exfat\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::EXT2 => CStr::from_bytes_with_nul(b"ext2\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::EXT3 => CStr::from_bytes_with_nul(b"ext3\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::EXT4 => CStr::from_bytes_with_nul(b"ext4\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::F2FS => CStr::from_bytes_with_nul(b"f2fs\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::FAT16 => CStr::from_bytes_with_nul(b"fat16\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::FAT32 => CStr::from_bytes_with_nul(b"fat32\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::NONE => CStr::from_bytes_with_nul(b"none\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::NTFS => CStr::from_bytes_with_nul(b"ntfs\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::SWAP => CStr::from_bytes_with_nul(b"swap\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::XFS => CStr::from_bytes_with_nul(b"xfs\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::LVM => CStr::from_bytes_with_nul(b"lvm\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM::LUKS => CStr::from_bytes_with_nul(b"luks\0").unwrap().as_ptr(),
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_strfilesys(fs: DISTINST_FILE_SYSTEM) -> *const libc::c_char {
    fs.get_cstr()
}
