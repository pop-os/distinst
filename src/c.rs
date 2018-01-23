extern crate libc;

use self::libc::{size_t, uint64_t, uint8_t};
use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::mem;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;
use std::slice;
use super::{log, Bootloader, Config, Disk, Disks, Error, FileSystemType, Installer,
            PartitionBuilder, PartitionFlag, PartitionInfo, PartitionTable, PartitionType, Sector,
            Status, Step};

/// Log level
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum DISTINST_LOG_LEVEL {
    TRACE,
    DEBUG,
    INFO,
    WARN,
    ERROR,
}

/// Installer log callback
pub type DistinstLogCallback = extern "C" fn(
    level: DISTINST_LOG_LEVEL,
    message: *const libc::c_char,
    user_data: *mut libc::c_void,
);

/// Bootloader steps
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum DISTINST_STEP {
    INIT,
    PARTITION,
    EXTRACT,
    CONFIGURE,
    BOOTLOADER,
}

impl From<DISTINST_STEP> for Step {
    fn from(step: DISTINST_STEP) -> Self {
        use DISTINST_STEP::*;
        match step {
            INIT => Step::Init,
            PARTITION => Step::Partition,
            EXTRACT => Step::Extract,
            CONFIGURE => Step::Configure,
            BOOTLOADER => Step::Bootloader,
        }
    }
}

impl From<Step> for DISTINST_STEP {
    fn from(step: Step) -> Self {
        use DISTINST_STEP::*;
        match step {
            Step::Init => INIT,
            Step::Partition => PARTITION,
            Step::Extract => EXTRACT,
            Step::Configure => CONFIGURE,
            Step::Bootloader => BOOTLOADER,
        }
    }
}

/// Installer configuration
#[repr(C)]
#[derive(Debug)]
pub struct DistinstConfig {
    squashfs: *const libc::c_char,
    lang: *const libc::c_char,
    remove: *const libc::c_char,
}

impl DistinstConfig {
    unsafe fn into_config(&self) -> Result<Config, io::Error> {
        if self.squashfs.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "config.squashfs: null pointer",
            ));
        }

        let squashfs = CStr::from_ptr(self.squashfs).to_str().map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("config.squashfs: invalid UTF-8: {}", err),
            )
        })?;

        if self.lang.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "config.lang: null pointer",
            ));
        }

        let lang = CStr::from_ptr(self.lang).to_str().map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("config.lang: invalid UTF-8: {}", err),
            )
        })?;

        if self.remove.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "config.remove: null pointer",
            ));
        }

        let remove = CStr::from_ptr(self.remove).to_str().map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("config.remove: invalid UTF-8: {}", err),
            )
        })?;

        Ok(Config {
            squashfs: squashfs.to_string(),
            lang: lang.to_string(),
            remove: remove.to_string(),
        })
    }
}

/// Installer error message
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DistinstError {
    step: DISTINST_STEP,
    err: libc::c_int,
}

/// Installer error callback
pub type DistinstErrorCallback =
    extern "C" fn(status: *const DistinstError, user_data: *mut libc::c_void);

/// Installer status message
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DistinstStatus {
    step: DISTINST_STEP,
    percent: libc::c_int,
}

/// Installer status callback
pub type DistinstStatusCallback =
    extern "C" fn(status: *const DistinstStatus, user_data: *mut libc::c_void);

/// An installer object
#[repr(C)]
pub struct DistinstInstaller;

/// Initialize logging
#[no_mangle]
pub unsafe extern "C" fn distinst_log(
    callback: DistinstLogCallback,
    user_data: *mut libc::c_void,
) -> libc::c_int {
    use DISTINST_LOG_LEVEL::*;
    use log::LogLevel;

    let user_data_sync = user_data as usize;
    match log(move |level, message| {
        let c_level = match level {
            LogLevel::Trace => TRACE,
            LogLevel::Debug => DEBUG,
            LogLevel::Info => INFO,
            LogLevel::Warn => WARN,
            LogLevel::Error => ERROR,
        };
        let c_message = CString::new(message).unwrap();
        callback(
            c_level,
            c_message.as_ptr(),
            user_data_sync as *mut libc::c_void,
        );
    }) {
        Ok(()) => 0,
        Err(_err) => libc::EINVAL,
    }
}

/// Create an installer object
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_new() -> *mut DistinstInstaller {
    Box::into_raw(Box::new(Installer::new())) as *mut DistinstInstaller
}

/// Send an installer status message
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_emit_error(
    installer: *mut DistinstInstaller,
    error: *const DistinstError,
) {
    (*(installer as *mut Installer)).emit_error(&Error {
        step: (*error).step.into(),
        err: io::Error::from_raw_os_error((*error).err),
    });
}

/// Set the installer status callback
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_on_error(
    installer: *mut DistinstInstaller,
    callback: DistinstErrorCallback,
    user_data: *mut libc::c_void,
) {
    (*(installer as *mut Installer)).on_error(move |error| {
        callback(
            &DistinstError {
                step: error.step.into(),
                err: error.err.raw_os_error().unwrap_or(libc::EIO),
            } as *const DistinstError,
            user_data,
        )
    });
}

/// Send an installer status message
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_emit_status(
    installer: *mut DistinstInstaller,
    status: *const DistinstStatus,
) {
    (*(installer as *mut Installer)).emit_status(&Status {
        step: (*status).step.into(),
        percent: (*status).percent,
    });
}

/// Set the installer status callback
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_on_status(
    installer: *mut DistinstInstaller,
    callback: DistinstStatusCallback,
    user_data: *mut libc::c_void,
) {
    (*(installer as *mut Installer)).on_status(move |status| {
        callback(
            &DistinstStatus {
                step: status.step.into(),
                percent: status.percent,
            } as *const DistinstStatus,
            user_data,
        )
    });
}

/// Install using this installer
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_install(
    installer: *mut DistinstInstaller,
    disk: *mut DistinstDisks,
    config: *const DistinstConfig,
) -> libc::c_int {
    let disks = if disk.is_null() {
        return libc::EIO;
    } else {
        let disks = DistinstDisks::from(*Box::from_raw(disk));
        Vec::from_raw_parts(disks.disks, disks.length, disks.length)
            .into_iter()
            .map(Disk::from)
            .collect()
    };

    match (*config).into_config() {
        Ok(config) => match (*(installer as *mut Installer)).install(disks, &config) {
            Ok(()) => 0,
            Err(err) => {
                info!("Install error: {}", err);
                err.raw_os_error().unwrap_or(libc::EIO)
            }
        },
        Err(err) => {
            info!("Config error: {}", err);
            let errno = err.raw_os_error().unwrap_or(libc::EIO);
            (*(installer as *mut Installer)).emit_error(&Error {
                step: Step::Init,
                err: err,
            });
            errno
        }
    }
}

/// Destroy an installer object
#[no_mangle]
pub unsafe extern "C" fn distinst_installer_destroy(installer: *mut DistinstInstaller) {
    drop(Box::from_raw(installer as *mut Installer))
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum DISTINST_PARTITION_TABLE {
    NONE = 0,
    GPT = 1,
    MSDOS = 2,
}

#[no_mangle]
pub unsafe extern "C" fn distinst_bootloader_detect() -> DISTINST_PARTITION_TABLE {
    match Bootloader::detect() {
        Bootloader::Bios => DISTINST_PARTITION_TABLE::MSDOS,
        Bootloader::Efi => DISTINST_PARTITION_TABLE::GPT,
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DISTINST_PARTITION_TYPE {
    PRIMARY = 1,
    LOGICAL = 2,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum DISTINST_PARTITION_FLAG {
    BOOT,
    ROOT,
    SWAP,
    HIDDEN,
    RAID,
    LVM,
    LBA,
    HPSERVICE,
    PALO,
    PREP,
    MSFT_RESERVED,
    BIOS_GRUB,
    APPLE_TV_RECOVERY,
    DIAG,
    LEGACY_BOOT,
    MSFT_DATA,
    IRST,
    ESP,
}

impl From<PartitionFlag> for DISTINST_PARTITION_FLAG {
    fn from(flag: PartitionFlag) -> DISTINST_PARTITION_FLAG {
        match flag {
            PartitionFlag::PED_PARTITION_BOOT => DISTINST_PARTITION_FLAG::BOOT,
            PartitionFlag::PED_PARTITION_ROOT => DISTINST_PARTITION_FLAG::ROOT,
            PartitionFlag::PED_PARTITION_SWAP => DISTINST_PARTITION_FLAG::SWAP,
            PartitionFlag::PED_PARTITION_HIDDEN => DISTINST_PARTITION_FLAG::HIDDEN,
            PartitionFlag::PED_PARTITION_RAID => DISTINST_PARTITION_FLAG::RAID,
            PartitionFlag::PED_PARTITION_LVM => DISTINST_PARTITION_FLAG::LVM,
            PartitionFlag::PED_PARTITION_LBA => DISTINST_PARTITION_FLAG::LBA,
            PartitionFlag::PED_PARTITION_HPSERVICE => DISTINST_PARTITION_FLAG::HPSERVICE,
            PartitionFlag::PED_PARTITION_PALO => DISTINST_PARTITION_FLAG::PALO,
            PartitionFlag::PED_PARTITION_PREP => DISTINST_PARTITION_FLAG::PREP,
            PartitionFlag::PED_PARTITION_MSFT_RESERVED => DISTINST_PARTITION_FLAG::MSFT_RESERVED,
            PartitionFlag::PED_PARTITION_BIOS_GRUB => DISTINST_PARTITION_FLAG::BIOS_GRUB,
            PartitionFlag::PED_PARTITION_APPLE_TV_RECOVERY => {
                DISTINST_PARTITION_FLAG::APPLE_TV_RECOVERY
            }
            PartitionFlag::PED_PARTITION_DIAG => DISTINST_PARTITION_FLAG::DIAG,
            PartitionFlag::PED_PARTITION_LEGACY_BOOT => DISTINST_PARTITION_FLAG::LEGACY_BOOT,
            PartitionFlag::PED_PARTITION_MSFT_DATA => DISTINST_PARTITION_FLAG::MSFT_DATA,
            PartitionFlag::PED_PARTITION_IRST => DISTINST_PARTITION_FLAG::IRST,
            PartitionFlag::PED_PARTITION_ESP => DISTINST_PARTITION_FLAG::ESP,
        }
    }
}

impl From<DISTINST_PARTITION_FLAG> for PartitionFlag {
    fn from(flag: DISTINST_PARTITION_FLAG) -> PartitionFlag {
        match flag {
            DISTINST_PARTITION_FLAG::BOOT => PartitionFlag::PED_PARTITION_BOOT,
            DISTINST_PARTITION_FLAG::ROOT => PartitionFlag::PED_PARTITION_ROOT,
            DISTINST_PARTITION_FLAG::SWAP => PartitionFlag::PED_PARTITION_SWAP,
            DISTINST_PARTITION_FLAG::HIDDEN => PartitionFlag::PED_PARTITION_HIDDEN,
            DISTINST_PARTITION_FLAG::RAID => PartitionFlag::PED_PARTITION_RAID,
            DISTINST_PARTITION_FLAG::LVM => PartitionFlag::PED_PARTITION_LVM,
            DISTINST_PARTITION_FLAG::LBA => PartitionFlag::PED_PARTITION_LBA,
            DISTINST_PARTITION_FLAG::HPSERVICE => PartitionFlag::PED_PARTITION_HPSERVICE,
            DISTINST_PARTITION_FLAG::PALO => PartitionFlag::PED_PARTITION_PALO,
            DISTINST_PARTITION_FLAG::PREP => PartitionFlag::PED_PARTITION_PREP,
            DISTINST_PARTITION_FLAG::MSFT_RESERVED => PartitionFlag::PED_PARTITION_MSFT_RESERVED,
            DISTINST_PARTITION_FLAG::BIOS_GRUB => PartitionFlag::PED_PARTITION_BIOS_GRUB,
            DISTINST_PARTITION_FLAG::APPLE_TV_RECOVERY => {
                PartitionFlag::PED_PARTITION_APPLE_TV_RECOVERY
            }
            DISTINST_PARTITION_FLAG::DIAG => PartitionFlag::PED_PARTITION_DIAG,
            DISTINST_PARTITION_FLAG::LEGACY_BOOT => PartitionFlag::PED_PARTITION_LEGACY_BOOT,
            DISTINST_PARTITION_FLAG::MSFT_DATA => PartitionFlag::PED_PARTITION_MSFT_DATA,
            DISTINST_PARTITION_FLAG::IRST => PartitionFlag::PED_PARTITION_IRST,
            DISTINST_PARTITION_FLAG::ESP => PartitionFlag::PED_PARTITION_ESP,
        }
    }
}

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

#[repr(C)]
pub struct DistinstDisks {
    disks: *mut DistinstDisk,
    length: size_t,
    capacity: size_t,
}

impl Drop for DistinstDisks {
    fn drop(&mut self) {
        drop(unsafe { Vec::from_raw_parts(self.disks, self.length, self.capacity) });
    }
}

/// Probes the disk for information about every disk in the device.
///
/// On error, a null pointer will be returned.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_new() -> *mut DistinstDisks {
    match Disks::probe_devices() {
        Ok(pdisks) => {
            let mut pdisks = pdisks
                .0
                .into_iter()
                .map(DistinstDisk::from)
                .collect::<Vec<DistinstDisk>>();

            pdisks.shrink_to_fit();
            let new_disks = DistinstDisks {
                disks: pdisks.as_mut_ptr(),
                length: pdisks.len(),
                capacity: pdisks.len(),
            };

            mem::forget(pdisks);
            Box::into_raw(Box::new(new_disks))
        }
        Err(why) => {
            info!("unable to probe devices: {}", why);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_with_capacity(length: size_t) -> *mut DistinstDisks {
    let mut vector: Vec<DistinstDisk> = Vec::with_capacity(length as usize);
    let disks = vector.as_mut_ptr();
    let length = vector.len();
    let capacity = vector.capacity();
    mem::forget(vector);
    Box::into_raw(Box::new(DistinstDisks {
        disks,
        length,
        capacity,
    }))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_push(
    disks: *mut *mut DistinstDisks,
    disk: *mut DistinstDisk,
) {
    let new_disks: *mut DistinstDisks = {
        let disks = Box::from_raw(*disks);
        let mut disks: Vec<DistinstDisk> =
            Vec::from_raw_parts(disks.disks, disks.length, disks.capacity);
        disks.push(*Box::from_raw(disk));

        let mut new_disks = disks.as_mut_ptr();
        let length = disks.len();
        let capacity = disks.capacity();
        mem::forget(disks);

        let new_disks = DistinstDisks {
            disks: new_disks,
            length,
            capacity,
        };

        Box::into_raw(Box::new(new_disks))
    };

    *disks = new_disks;
}

/// The deconstructor for a `DistinstDisks`.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_destroy(disks: *mut DistinstDisks) {
    if !disks.is_null() {
        drop(Box::from_raw(disks))
    }
}

#[repr(C)]
pub struct DistinstDisk {
    model_name: *mut libc::c_char,
    serial: *mut libc::c_char,
    device_path: *mut libc::c_char,
    device_type: *mut libc::c_char,
    sectors: uint64_t,
    sector_size: uint64_t,
    partitions: DistinstPartitions,
    table_type: DISTINST_PARTITION_TABLE,
    read_only: uint8_t,
}

impl Clone for DistinstDisk {
    fn clone(&self) -> DistinstDisk {
        DistinstDisk {
            model_name: clone_cstr(self.model_name),
            serial: clone_cstr(self.serial),
            device_path: clone_cstr(self.device_path),
            device_type: clone_cstr(self.device_type),
            sectors: self.sectors,
            sector_size: self.sector_size,
            partitions: self.partitions.clone(),
            table_type: self.table_type,
            read_only: self.read_only,
        }
    }
}

impl Drop for DistinstDisk {
    fn drop(&mut self) {
        unsafe {
            drop(CString::from_raw(self.model_name));
            drop(CString::from_raw(self.serial));
            drop(CString::from_raw(self.device_type));
            drop(CString::from_raw(self.device_path));
            let length = self.partitions.length;
            drop(Vec::from_raw_parts(self.partitions.parts, length, length));
        }
    }
}

impl From<Disk> for DistinstDisk {
    fn from(disk: Disk) -> DistinstDisk {
        let mut parts: Vec<DistinstPartition> = disk.partitions
            .into_iter()
            .map(DistinstPartition::from)
            .collect();
        parts.shrink_to_fit();
        let partitions = DistinstPartitions {
            parts: parts.as_mut_ptr(),
            length: parts.len(),
        };

        mem::forget(parts);
        DistinstDisk {
            model_name: from_string_to_ptr(disk.model_name),
            serial: from_string_to_ptr(disk.serial),
            device_path: from_path_to_ptr(disk.device_path),
            device_type: from_string_to_ptr(disk.device_type),
            sectors: disk.size as libc::c_ulong,
            sector_size: disk.sector_size,
            table_type: match disk.table_type {
                None => DISTINST_PARTITION_TABLE::NONE,
                Some(PartitionTable::Msdos) => DISTINST_PARTITION_TABLE::MSDOS,
                Some(PartitionTable::Gpt) => DISTINST_PARTITION_TABLE::GPT,
            },
            read_only: if disk.read_only { 1 } else { 0 },
            partitions,
        }
    }
}

impl From<DistinstDisk> for Disk {
    fn from(disk: DistinstDisk) -> Disk {
        let (parts, plen) = (disk.partitions.parts, disk.partitions.length);

        Disk {
            model_name: from_ptr_to_string(disk.model_name),
            serial: from_ptr_to_string(disk.serial),
            device_path: from_ptr_to_path(disk.device_path),
            size: disk.sectors as u64,
            sector_size: disk.sector_size as u64,
            device_type: from_ptr_to_string(disk.device_type),
            table_type: match disk.table_type {
                DISTINST_PARTITION_TABLE::GPT => Some(PartitionTable::Gpt),
                DISTINST_PARTITION_TABLE::MSDOS => Some(PartitionTable::Msdos),
                DISTINST_PARTITION_TABLE::NONE => None,
            },
            read_only: disk.read_only != 0,
            partitions: unsafe { Vec::from_raw_parts(parts, plen, plen) }
                .into_iter()
                .map(PartitionInfo::from)
                .collect::<Vec<_>>(),
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct DistinstSector {
    flag: DISTINST_SECTOR_KIND,
    value: uint64_t,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub enum DISTINST_SECTOR_KIND {
    START = 1,
    END = 2,
    UNIT = 3,
    MEGABYTE = 4,
    PERCENT = 5,
}

impl From<DistinstSector> for Sector {
    fn from(sector: DistinstSector) -> Sector {
        match sector.flag {
            DISTINST_SECTOR_KIND::START => Sector::Start,
            DISTINST_SECTOR_KIND::END => Sector::End,
            DISTINST_SECTOR_KIND::UNIT => Sector::Unit(sector.value as u64),
            DISTINST_SECTOR_KIND::MEGABYTE => Sector::Megabyte(sector.value as u64),
            DISTINST_SECTOR_KIND::PERCENT => {
                debug_assert!(sector.value <= ::std::u16::MAX as u64);
                Sector::Percent(sector.value as u16)
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_start() -> DistinstSector {
    DistinstSector {
        flag: DISTINST_SECTOR_KIND::START,
        value: 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_end() -> DistinstSector {
    DistinstSector {
        flag: DISTINST_SECTOR_KIND::END,
        value: 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_megabyte(value: uint64_t) -> DistinstSector {
    DistinstSector {
        flag: DISTINST_SECTOR_KIND::MEGABYTE,
        value,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_unit(value: uint64_t) -> DistinstSector {
    DistinstSector {
        flag: DISTINST_SECTOR_KIND::UNIT,
        value,
    }
}

/// Obtains a specific disk's information by the device path.
///
/// On an error, this will return a null pointer.
#[no_mangle]
pub unsafe extern "C" fn distinst_disk_new(path: *const libc::c_char) -> *mut DistinstDisk {
    if path.is_null() {
        return ptr::null_mut();
    }
    let cstring = CStr::from_ptr(path);
    let ostring = OsStr::from_bytes(cstring.to_bytes());
    match Disk::from_name(ostring).map(DistinstDisk::from) {
        Ok(disk) => Box::into_raw(Box::new(disk)),
        Err(why) => {
            info!(
                "unable to open device at {}: {}",
                ostring.to_string_lossy(),
                why
            );
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_sector(
    disk: *const DistinstDisk,
    sector: *const DistinstSector,
) -> uint64_t {
    Disk::from((*disk).clone()).get_sector(Sector::from(*sector))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_mklabel(
    disk: *mut *mut DistinstDisk,
    table: DISTINST_PARTITION_TABLE,
) -> libc::c_int {
    let table = match table {
        DISTINST_PARTITION_TABLE::GPT => PartitionTable::Gpt,
        DISTINST_PARTITION_TABLE::MSDOS => PartitionTable::Msdos,
        _ => return 1,
    };

    disk_action(disk, |disk| {
        if let Err(why) = disk.mklabel(table) {
            info!(
                "unable to write partition table on {}: {}",
                disk.path().display(),
                why
            );
            1
        } else {
            0
        }
    })
}

/// A destructor for a `DistinstDisk`
#[no_mangle]
pub unsafe extern "C" fn distinst_disk_destroy(disk: *mut DistinstDisk) {
    drop(Box::from_raw(disk))
}

/// Converts a `DistinstDisk` into a `Disk`, executes a given action with that `Disk`,
/// then converts it back into a `DistinstDisk`, returning the exit status of the function.
unsafe fn disk_action<F: Fn(&mut Disk) -> libc::c_int>(
    disk: *mut *mut DistinstDisk,
    action: F,
) -> libc::c_int {
    let mut new_disk = Disk::from(*Box::from_raw(*disk));
    let exit_status = action(&mut new_disk);
    *disk = Box::into_raw(Box::new(DistinstDisk::from(new_disk)));
    exit_status
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_add_partition(
    disk: *mut *mut DistinstDisk,
    partition: *mut DistinstPartitionBuilder,
) -> libc::c_int {
    disk_action(disk, |disk| {
        if let Err(why) = disk.add_partition(PartitionBuilder::from(*Box::from_raw(partition))) {
            info!("unable to add partition: {}", why);
            1
        } else {
            0
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_remove_partition(
    disk: *mut *mut DistinstDisk,
    partition: libc::c_int,
) -> libc::c_int {
    disk_action(disk, |disk| {
        if let Err(why) = disk.remove_partition(partition) {
            info!("unable to remove partition: {}", why);
            1
        } else {
            0
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_resize_partition(
    disk: *mut *mut DistinstDisk,
    partition: libc::c_int,
    length: uint64_t,
) -> libc::c_int {
    disk_action(disk, |disk| {
        if let Err(why) = disk.resize_partition(partition, length) {
            info!("unable to resize partition: {}", why);
            1
        } else {
            0
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_move_partition(
    disk: *mut *mut DistinstDisk,
    partition: libc::c_int,
    start: uint64_t,
) -> libc::c_int {
    disk_action(disk, |disk| {
        if let Err(why) = disk.move_partition(partition, start) {
            info!("unable to remove partition: {}", why);
            1
        } else {
            0
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_format_partition(
    disk: *mut *mut DistinstDisk,
    partition: libc::c_int,
    fs: DISTINST_FILE_SYSTEM_TYPE,
) -> libc::c_int {
    let fs = match Option::<FileSystemType>::from(fs) {
        Some(fs) => fs,
        None => {
            info!("file system type required");
            return 1;
        }
    };

    disk_action(disk, |disk| {
        if let Err(why) = disk.format_partition(partition, fs) {
            info!("unable to remove partition: {}", why);
            1
        } else {
            0
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_commit(disk: *mut *mut DistinstDisk) -> libc::c_int {
    disk_action(disk, |disk| {
        if let Err(why) = disk.commit() {
            info!("unable to commit changes to disk: {}", why);
            1
        } else {
            0
        }
    })
}

#[repr(C)]
pub struct DistinstPartitionBuilder {
    start_sector: uint64_t,
    end_sector: uint64_t,
    filesystem: DISTINST_FILE_SYSTEM_TYPE,
    part_type: DISTINST_PARTITION_TYPE,
    name: *mut libc::c_char,
    target: *mut libc::c_char,
    flags: DistinstPartitionFlags,
}

impl Drop for DistinstPartitionBuilder {
    fn drop(&mut self) {
        if !self.name.is_null() {
            drop(unsafe { CString::from_raw(self.name) });
        }
    }
}

impl From<DistinstPartitionBuilder> for PartitionBuilder {
    fn from(distinst: DistinstPartitionBuilder) -> PartitionBuilder {
        debug_assert!(distinst.filesystem != DISTINST_FILE_SYSTEM_TYPE::NONE);

        PartitionBuilder {
            start_sector: distinst.start_sector as u64,
            end_sector: distinst.end_sector as u64,
            filesystem: Option::<FileSystemType>::from(distinst.filesystem).unwrap(),
            part_type: match distinst.part_type {
                DISTINST_PARTITION_TYPE::LOGICAL => PartitionType::Logical,
                DISTINST_PARTITION_TYPE::PRIMARY => PartitionType::Primary,
            },
            name: if distinst.name.is_null() {
                None
            } else {
                match String::from_utf8(unsafe { CString::from_raw(distinst.name).into_bytes() }) {
                    Ok(name) => Some(name),
                    Err(why) => {
                        info!("partition name was not valid UTF-8: {}", why);
                        None
                    }
                }
            },
            flags: unsafe {
                Vec::from_raw_parts(
                    distinst.flags.flags,
                    distinst.flags.length,
                    distinst.flags.capacity,
                ).into_iter()
                    .map(PartitionFlag::from)
                    .collect()
            },
            mount: if distinst.target.is_null() {
                None
            } else {
                Some(from_ptr_to_path(distinst.target))
            },
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_destroy(
    builder: *mut DistinstPartitionBuilder,
) {
    drop(Box::from_raw(builder));
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_new(
    start_sector: uint64_t,
    end_sector: uint64_t,
    filesystem: DISTINST_FILE_SYSTEM_TYPE,
) -> *mut DistinstPartitionBuilder {
    let mut vec = Vec::with_capacity(8);
    let flags = vec.as_mut_ptr();
    let capacity = vec.capacity();
    mem::forget(vec);

    let builder = DistinstPartitionBuilder {
        start_sector,
        end_sector: end_sector - 1,
        filesystem,
        part_type: DISTINST_PARTITION_TYPE::PRIMARY,
        name: ptr::null_mut(),
        target: ptr::null_mut(),
        flags: DistinstPartitionFlags {
            flags,
            length: 0,
            capacity,
        },
    };

    Box::into_raw(Box::new(builder))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_set_name(
    builder: *mut DistinstPartitionBuilder,
    name: *mut libc::c_char,
) -> *mut DistinstPartitionBuilder {
    (*builder).name = name;
    builder
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_set_mount(
    builder: *mut DistinstPartitionBuilder,
    target: *mut libc::c_char,
) -> *mut DistinstPartitionBuilder {
    (*builder).target = target;
    builder
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_set_partition_type(
    builder: *mut DistinstPartitionBuilder,
    part_type: DISTINST_PARTITION_TYPE,
) -> *mut DistinstPartitionBuilder {
    (*builder).part_type = part_type;
    builder
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_add_flag(
    builder: *mut DistinstPartitionBuilder,
    flag: DISTINST_PARTITION_FLAG,
) -> *mut DistinstPartitionBuilder {
    let mut flags = Vec::from_raw_parts(
        (*builder).flags.flags,
        (*builder).flags.length,
        (*builder).flags.capacity,
    );
    flags.push(flag);
    (*builder).flags.length = flags.len();
    (*builder).flags.capacity = flags.capacity();
    (*builder).flags.flags = flags.as_mut_ptr();
    mem::forget(flags);
    builder
}

#[repr(C)]
pub struct DistinstPartition {
    is_source: uint8_t,
    remove: uint8_t,
    format: uint8_t,
    active: uint8_t,
    busy: uint8_t,
    part_type: DISTINST_PARTITION_TYPE,
    filesystem: DISTINST_FILE_SYSTEM_TYPE,
    number: libc::int32_t,
    start_sector: uint64_t,
    end_sector: uint64_t,
    flags: DistinstPartitionFlags,
    name: *mut libc::c_char,
    device_path: *mut libc::c_char,
    mount_point: *mut libc::c_char,
    target: *mut libc::c_char,
}

impl Clone for DistinstPartition {
    fn clone(&self) -> DistinstPartition {
        DistinstPartition {
            is_source: self.is_source,
            remove: self.remove,
            format: self.format,
            active: self.active,
            busy: self.busy,
            part_type: self.part_type,
            filesystem: self.filesystem,
            number: self.number,
            start_sector: self.start_sector,
            end_sector: self.end_sector,
            flags: self.flags.clone(),
            name: clone_cstr(self.name),
            device_path: clone_cstr(self.device_path),
            mount_point: clone_cstr(self.mount_point),
            target: clone_cstr(self.target),
        }
    }
}

impl From<PartitionInfo> for DistinstPartition {
    fn from(part: PartitionInfo) -> DistinstPartition {
        let mut pflags: Vec<DISTINST_PARTITION_FLAG> = part.flags
            .into_iter()
            .map(DISTINST_PARTITION_FLAG::from)
            .collect();
        pflags.shrink_to_fit();

        let flags = DistinstPartitionFlags {
            flags: pflags.as_mut_ptr(),
            length: pflags.len(),
            capacity: pflags.capacity(),
        };

        mem::forget(pflags);
        DistinstPartition {
            is_source: if part.is_source { 1 } else { 0 },
            remove: if part.remove { 1 } else { 0 },
            format: if part.format { 1 } else { 0 },
            active: if part.active { 1 } else { 0 },
            busy: if part.busy { 1 } else { 0 },
            number: part.number as libc::int32_t,
            start_sector: part.start_sector as uint64_t,
            end_sector: part.end_sector as uint64_t,
            part_type: match part.part_type {
                PartitionType::Logical => DISTINST_PARTITION_TYPE::LOGICAL,
                PartitionType::Primary => DISTINST_PARTITION_TYPE::PRIMARY,
            },
            filesystem: part.filesystem.map_or(
                DISTINST_FILE_SYSTEM_TYPE::NONE,
                DISTINST_FILE_SYSTEM_TYPE::from,
            ),
            flags,
            name: part.name.map_or(ptr::null_mut(), from_string_to_ptr),
            device_path: from_path_to_ptr(part.device_path),
            mount_point: part.mount_point.map_or(ptr::null_mut(), from_path_to_ptr),
            target: part.target.map_or(ptr::null_mut(), from_path_to_ptr),
        }
    }
}

impl From<DistinstPartition> for PartitionInfo {
    fn from(part: DistinstPartition) -> PartitionInfo {
        let (flags, flen) = (part.flags.flags, part.flags.length);
        PartitionInfo {
            is_source: part.is_source != 0,
            remove: part.remove != 0,
            format: part.format != 0,
            active: part.active != 0,
            busy: part.busy != 0,
            number: part.number as i32,
            start_sector: part.start_sector as u64,
            end_sector: part.end_sector as u64,
            part_type: match part.part_type {
                DISTINST_PARTITION_TYPE::LOGICAL => PartitionType::Logical,
                DISTINST_PARTITION_TYPE::PRIMARY => PartitionType::Primary,
            },
            filesystem: Option::<FileSystemType>::from(part.filesystem),
            flags: unsafe {
                Vec::from_raw_parts(flags, flen, flen)
                    .into_iter()
                    .map(PartitionFlag::from)
                    .collect()
            },
            name: if part.name.is_null() {
                None
            } else {
                Some(from_ptr_to_string(part.name))
            },
            device_path: from_ptr_to_path(part.device_path),
            mount_point: if part.mount_point.is_null() {
                None
            } else {
                Some(from_ptr_to_path(part.mount_point))
            },
            target: if part.target.is_null() {
                None
            } else {
                Some(from_ptr_to_path(part.target))
            },
        }
    }
}

#[repr(C)]
pub struct DistinstPartitionFlags {
    flags: *mut DISTINST_PARTITION_FLAG,
    length: size_t,
    capacity: size_t,
}

impl Clone for DistinstPartitionFlags {
    fn clone(&self) -> Self {
        DistinstPartitionFlags {
            flags: unsafe {
                let mut vec = slice::from_raw_parts(self.flags, self.length).to_owned();
                let ptr = vec.as_mut_ptr();
                mem::forget(vec);
                ptr
            },
            length: self.length,
            capacity: self.capacity,
        }
    }
}

impl Drop for DistinstPartitionFlags {
    fn drop(&mut self) {
        drop(unsafe { Vec::from_raw_parts(self.flags, self.length, self.capacity) });
    }
}

#[repr(C)]
pub struct DistinstPartitions {
    parts: *mut DistinstPartition,
    length: size_t,
}

impl Clone for DistinstPartitions {
    fn clone(&self) -> Self {
        DistinstPartitions {
            parts: unsafe {
                let mut vec = slice::from_raw_parts(self.parts, self.length).to_owned();
                let ptr = vec.as_mut_ptr();
                mem::forget(vec);
                ptr
            },
            length: self.length,
        }
    }
}

impl Drop for DistinstPartitions {
    fn drop(&mut self) {
        drop(unsafe { Vec::from_raw_parts(self.parts, self.length, self.length) });
    }
}

/// Should only be used internally to recover strings that were converted into pointers.
fn from_ptr_to_string(pointer: *mut libc::c_char) -> String {
    unsafe { String::from_utf8_unchecked(CString::from_raw(pointer).into_bytes()) }
}

/// Converts a Rust string into a C-native char array.
fn from_string_to_ptr(mut string: String) -> *mut libc::c_char {
    string.shrink_to_fit();
    CString::new(string)
        .ok()
        .map_or(ptr::null_mut(), |string| string.into_raw())
}

/// Should only be used internally to recover paths that were converted into pointers.
fn from_ptr_to_path(pointer: *mut libc::c_char) -> PathBuf {
    unsafe {
        PathBuf::from(String::from_utf8_unchecked(
            CString::from_raw(pointer).into_bytes(),
        ))
    }
}

/// Converts a Rust path into a C-native char array.
fn from_path_to_ptr(path: PathBuf) -> *mut libc::c_char {
    path.to_str()
        .and_then(|string| CString::new(string).ok())
        .map_or(ptr::null_mut(), |string| string.into_raw())
}

fn clone_cstr(string: *const libc::c_char) -> *mut libc::c_char {
    if string.is_null() {
        ptr::null_mut()
    } else {
        unsafe { CStr::from_ptr(string).to_owned().into_raw() }
    }
}
