use libc;

use std::ffi::CStr;

use FileSystemType;

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
}

impl From<DISTINST_FILE_SYSTEM_TYPE> for Option<FileSystemType> {
    fn from(fs: DISTINST_FILE_SYSTEM_TYPE) -> Option<FileSystemType> {
        match fs {
            DISTINST_FILE_SYSTEM_TYPE::BTRFS => Some(FileSystemType::Btrfs),
            DISTINST_FILE_SYSTEM_TYPE::EXFAT => Some(FileSystemType::Exfat),
            DISTINST_FILE_SYSTEM_TYPE::EXT2 => Some(FileSystemType::Ext2),
            DISTINST_FILE_SYSTEM_TYPE::EXT3 => Some(FileSystemType::Ext3),
            DISTINST_FILE_SYSTEM_TYPE::EXT4 => Some(FileSystemType::Ext4),
            DISTINST_FILE_SYSTEM_TYPE::F2FS => Some(FileSystemType::F2fs),
            DISTINST_FILE_SYSTEM_TYPE::FAT16 => Some(FileSystemType::Fat16),
            DISTINST_FILE_SYSTEM_TYPE::FAT32 => Some(FileSystemType::Fat32),
            DISTINST_FILE_SYSTEM_TYPE::NONE => None,
            DISTINST_FILE_SYSTEM_TYPE::NTFS => Some(FileSystemType::Ntfs),
            DISTINST_FILE_SYSTEM_TYPE::SWAP => Some(FileSystemType::Swap),
            DISTINST_FILE_SYSTEM_TYPE::XFS => Some(FileSystemType::Xfs),
        }
    }
}

impl From<FileSystemType> for DISTINST_FILE_SYSTEM_TYPE {
    fn from(fs: FileSystemType) -> DISTINST_FILE_SYSTEM_TYPE {
        match fs {
            FileSystemType::Btrfs => DISTINST_FILE_SYSTEM_TYPE::BTRFS,
            FileSystemType::Exfat => DISTINST_FILE_SYSTEM_TYPE::EXFAT,
            FileSystemType::Ext2 => DISTINST_FILE_SYSTEM_TYPE::EXT2,
            FileSystemType::Ext3 => DISTINST_FILE_SYSTEM_TYPE::EXT3,
            FileSystemType::Ext4 => DISTINST_FILE_SYSTEM_TYPE::EXT4,
            FileSystemType::F2fs => DISTINST_FILE_SYSTEM_TYPE::F2FS,
            FileSystemType::Fat16 => DISTINST_FILE_SYSTEM_TYPE::FAT16,
            FileSystemType::Fat32 => DISTINST_FILE_SYSTEM_TYPE::FAT32,
            FileSystemType::Ntfs => DISTINST_FILE_SYSTEM_TYPE::NTFS,
            FileSystemType::Swap => DISTINST_FILE_SYSTEM_TYPE::SWAP,
            FileSystemType::Xfs => DISTINST_FILE_SYSTEM_TYPE::XFS,
        }
    }
}

impl DISTINST_FILE_SYSTEM_TYPE {
    fn get_cstr(&self) -> *const libc::c_char {
        match *self {
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
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn strfilesys(fs: DISTINST_FILE_SYSTEM_TYPE) -> *const libc::c_char {
    fs.get_cstr()
}
