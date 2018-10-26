use libc;

use std::ffi::CStr;

use distinst::FileSystem;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DISTINST_FILE_SYSTEM_TYPE {
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

impl From<DISTINST_FILE_SYSTEM_TYPE> for Option<FileSystem> {
    fn from(fs: DISTINST_FILE_SYSTEM_TYPE) -> Option<FileSystem> {
        match fs {
            DISTINST_FILE_SYSTEM_TYPE::BTRFS => Some(FileSystem::Btrfs),
            DISTINST_FILE_SYSTEM_TYPE::EXFAT => Some(FileSystem::Exfat),
            DISTINST_FILE_SYSTEM_TYPE::EXT2 => Some(FileSystem::Ext2),
            DISTINST_FILE_SYSTEM_TYPE::EXT3 => Some(FileSystem::Ext3),
            DISTINST_FILE_SYSTEM_TYPE::EXT4 => Some(FileSystem::Ext4),
            DISTINST_FILE_SYSTEM_TYPE::F2FS => Some(FileSystem::F2fs),
            DISTINST_FILE_SYSTEM_TYPE::FAT16 => Some(FileSystem::Fat16),
            DISTINST_FILE_SYSTEM_TYPE::FAT32 => Some(FileSystem::Fat32),
            DISTINST_FILE_SYSTEM_TYPE::NONE => None,
            DISTINST_FILE_SYSTEM_TYPE::NTFS => Some(FileSystem::Ntfs),
            DISTINST_FILE_SYSTEM_TYPE::SWAP => Some(FileSystem::Swap),
            DISTINST_FILE_SYSTEM_TYPE::XFS => Some(FileSystem::Xfs),
            DISTINST_FILE_SYSTEM_TYPE::LVM => Some(FileSystem::Lvm),
            DISTINST_FILE_SYSTEM_TYPE::LUKS => Some(FileSystem::Luks),
        }
    }
}

impl From<FileSystem> for DISTINST_FILE_SYSTEM_TYPE {
    fn from(fs: FileSystem) -> DISTINST_FILE_SYSTEM_TYPE {
        match fs {
            FileSystem::Btrfs => DISTINST_FILE_SYSTEM_TYPE::BTRFS,
            FileSystem::Exfat => DISTINST_FILE_SYSTEM_TYPE::EXFAT,
            FileSystem::Ext2 => DISTINST_FILE_SYSTEM_TYPE::EXT2,
            FileSystem::Ext3 => DISTINST_FILE_SYSTEM_TYPE::EXT3,
            FileSystem::Ext4 => DISTINST_FILE_SYSTEM_TYPE::EXT4,
            FileSystem::F2fs => DISTINST_FILE_SYSTEM_TYPE::F2FS,
            FileSystem::Fat16 => DISTINST_FILE_SYSTEM_TYPE::FAT16,
            FileSystem::Fat32 => DISTINST_FILE_SYSTEM_TYPE::FAT32,
            FileSystem::Ntfs => DISTINST_FILE_SYSTEM_TYPE::NTFS,
            FileSystem::Swap => DISTINST_FILE_SYSTEM_TYPE::SWAP,
            FileSystem::Xfs => DISTINST_FILE_SYSTEM_TYPE::XFS,
            FileSystem::Lvm => DISTINST_FILE_SYSTEM_TYPE::LVM,
            FileSystem::Luks => DISTINST_FILE_SYSTEM_TYPE::LUKS,
        }
    }
}

impl DISTINST_FILE_SYSTEM_TYPE {
    fn get_cstr(self) -> *const libc::c_char {
        match self {
            DISTINST_FILE_SYSTEM_TYPE::BTRFS => {
                CStr::from_bytes_with_nul(b"btrfs\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::EXFAT => {
                CStr::from_bytes_with_nul(b"exfat\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::EXT2 => {
                CStr::from_bytes_with_nul(b"ext2\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::EXT3 => {
                CStr::from_bytes_with_nul(b"ext3\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::EXT4 => {
                CStr::from_bytes_with_nul(b"ext4\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::F2FS => {
                CStr::from_bytes_with_nul(b"f2fs\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::FAT16 => {
                CStr::from_bytes_with_nul(b"fat16\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::FAT32 => {
                CStr::from_bytes_with_nul(b"fat32\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::NONE => {
                CStr::from_bytes_with_nul(b"none\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::NTFS => {
                CStr::from_bytes_with_nul(b"ntfs\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::SWAP => {
                CStr::from_bytes_with_nul(b"swap\0").unwrap().as_ptr()
            }
            DISTINST_FILE_SYSTEM_TYPE::XFS => CStr::from_bytes_with_nul(b"xfs\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM_TYPE::LVM => CStr::from_bytes_with_nul(b"lvm\0").unwrap().as_ptr(),
            DISTINST_FILE_SYSTEM_TYPE::LUKS => {
                CStr::from_bytes_with_nul(b"luks\0").unwrap().as_ptr()
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_strfilesys(fs: DISTINST_FILE_SYSTEM_TYPE) -> *const libc::c_char {
    fs.get_cstr()
}
