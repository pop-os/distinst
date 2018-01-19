extern crate libc;

use self::libc::{size_t, uint64_t, uint8_t};
use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::mem;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;
use super::{log, Config, Disk, Disks, Error, FileSystemType, Installer, PartitionBuilder,
            PartitionFlag, PartitionInfo, PartitionTable, PartitionType, Status, Step};

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
pub enum PARTITION_TABLE {
    NONE = 0,
    GPT = 1,
    MSDOS = 2,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PARTITION_TYPE {
    PRIMARY = 1,
    LOGICAL = 2,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum PARTITION_FLAG {
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

impl From<PartitionFlag> for PARTITION_FLAG {
    fn from(flag: PartitionFlag) -> PARTITION_FLAG {
        match flag {
            PartitionFlag::PED_PARTITION_BOOT => PARTITION_FLAG::BOOT,
            PartitionFlag::PED_PARTITION_ROOT => PARTITION_FLAG::ROOT,
            PartitionFlag::PED_PARTITION_SWAP => PARTITION_FLAG::SWAP,
            PartitionFlag::PED_PARTITION_HIDDEN => PARTITION_FLAG::HIDDEN,
            PartitionFlag::PED_PARTITION_RAID => PARTITION_FLAG::RAID,
            PartitionFlag::PED_PARTITION_LVM => PARTITION_FLAG::LVM,
            PartitionFlag::PED_PARTITION_LBA => PARTITION_FLAG::LBA,
            PartitionFlag::PED_PARTITION_HPSERVICE => PARTITION_FLAG::HPSERVICE,
            PartitionFlag::PED_PARTITION_PALO => PARTITION_FLAG::PALO,
            PartitionFlag::PED_PARTITION_PREP => PARTITION_FLAG::PREP,
            PartitionFlag::PED_PARTITION_MSFT_RESERVED => PARTITION_FLAG::MSFT_RESERVED,
            PartitionFlag::PED_PARTITION_BIOS_GRUB => PARTITION_FLAG::BIOS_GRUB,
            PartitionFlag::PED_PARTITION_APPLE_TV_RECOVERY => PARTITION_FLAG::APPLE_TV_RECOVERY,
            PartitionFlag::PED_PARTITION_DIAG => PARTITION_FLAG::DIAG,
            PartitionFlag::PED_PARTITION_LEGACY_BOOT => PARTITION_FLAG::LEGACY_BOOT,
            PartitionFlag::PED_PARTITION_MSFT_DATA => PARTITION_FLAG::MSFT_DATA,
            PartitionFlag::PED_PARTITION_IRST => PARTITION_FLAG::IRST,
            PartitionFlag::PED_PARTITION_ESP => PARTITION_FLAG::ESP,
        }
    }
}

impl From<PARTITION_FLAG> for PartitionFlag {
    fn from(flag: PARTITION_FLAG) -> PartitionFlag {
        match flag {
            PARTITION_FLAG::BOOT => PartitionFlag::PED_PARTITION_BOOT,
            PARTITION_FLAG::ROOT => PartitionFlag::PED_PARTITION_ROOT,
            PARTITION_FLAG::SWAP => PartitionFlag::PED_PARTITION_SWAP,
            PARTITION_FLAG::HIDDEN => PartitionFlag::PED_PARTITION_HIDDEN,
            PARTITION_FLAG::RAID => PartitionFlag::PED_PARTITION_RAID,
            PARTITION_FLAG::LVM => PartitionFlag::PED_PARTITION_LVM,
            PARTITION_FLAG::LBA => PartitionFlag::PED_PARTITION_LBA,
            PARTITION_FLAG::HPSERVICE => PartitionFlag::PED_PARTITION_HPSERVICE,
            PARTITION_FLAG::PALO => PartitionFlag::PED_PARTITION_PALO,
            PARTITION_FLAG::PREP => PartitionFlag::PED_PARTITION_PREP,
            PARTITION_FLAG::MSFT_RESERVED => PartitionFlag::PED_PARTITION_MSFT_RESERVED,
            PARTITION_FLAG::BIOS_GRUB => PartitionFlag::PED_PARTITION_BIOS_GRUB,
            PARTITION_FLAG::APPLE_TV_RECOVERY => PartitionFlag::PED_PARTITION_APPLE_TV_RECOVERY,
            PARTITION_FLAG::DIAG => PartitionFlag::PED_PARTITION_DIAG,
            PARTITION_FLAG::LEGACY_BOOT => PartitionFlag::PED_PARTITION_LEGACY_BOOT,
            PARTITION_FLAG::MSFT_DATA => PartitionFlag::PED_PARTITION_MSFT_DATA,
            PARTITION_FLAG::IRST => PartitionFlag::PED_PARTITION_IRST,
            PARTITION_FLAG::ESP => PartitionFlag::PED_PARTITION_ESP,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FILE_SYSTEM {
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

impl From<FILE_SYSTEM> for Option<FileSystemType> {
    fn from(fs: FILE_SYSTEM) -> Option<FileSystemType> {
        match fs {
            FILE_SYSTEM::BTRFS => Some(FileSystemType::Btrfs),
            FILE_SYSTEM::EXFAT => Some(FileSystemType::Exfat),
            FILE_SYSTEM::EXT2 => Some(FileSystemType::Ext2),
            FILE_SYSTEM::EXT3 => Some(FileSystemType::Ext3),
            FILE_SYSTEM::EXT4 => Some(FileSystemType::Ext4),
            FILE_SYSTEM::F2FS => Some(FileSystemType::F2fs),
            FILE_SYSTEM::FAT16 => Some(FileSystemType::Fat16),
            FILE_SYSTEM::FAT32 => Some(FileSystemType::Fat32),
            FILE_SYSTEM::NONE => None,
            FILE_SYSTEM::NTFS => Some(FileSystemType::Ntfs),
            FILE_SYSTEM::SWAP => Some(FileSystemType::Swap),
            FILE_SYSTEM::XFS => Some(FileSystemType::Xfs),
        }
    }
}

impl From<FileSystemType> for FILE_SYSTEM {
    fn from(fs: FileSystemType) -> FILE_SYSTEM {
        match fs {
            FileSystemType::Btrfs => FILE_SYSTEM::BTRFS,
            FileSystemType::Exfat => FILE_SYSTEM::EXFAT,
            FileSystemType::Ext2 => FILE_SYSTEM::EXT2,
            FileSystemType::Ext3 => FILE_SYSTEM::EXT3,
            FileSystemType::Ext4 => FILE_SYSTEM::EXT4,
            FileSystemType::F2fs => FILE_SYSTEM::F2FS,
            FileSystemType::Fat16 => FILE_SYSTEM::FAT16,
            FileSystemType::Fat32 => FILE_SYSTEM::FAT32,
            FileSystemType::Ntfs => FILE_SYSTEM::NTFS,
            FileSystemType::Swap => FILE_SYSTEM::SWAP,
            FileSystemType::Xfs => FILE_SYSTEM::XFS,
        }
    }
}

impl FILE_SYSTEM {
    fn get_cstr(&self) -> *const libc::c_char {
        match *self {
            FILE_SYSTEM::BTRFS => CStr::from_bytes_with_nul(b"btrfs\0").unwrap().as_ptr(),
            FILE_SYSTEM::EXFAT => CStr::from_bytes_with_nul(b"exfat\0").unwrap().as_ptr(),
            FILE_SYSTEM::EXT2 => CStr::from_bytes_with_nul(b"ext2\0").unwrap().as_ptr(),
            FILE_SYSTEM::EXT3 => CStr::from_bytes_with_nul(b"ext3\0").unwrap().as_ptr(),
            FILE_SYSTEM::EXT4 => CStr::from_bytes_with_nul(b"ext4\0").unwrap().as_ptr(),
            FILE_SYSTEM::F2FS => CStr::from_bytes_with_nul(b"f2fs\0").unwrap().as_ptr(),
            FILE_SYSTEM::FAT16 => CStr::from_bytes_with_nul(b"fat16\0").unwrap().as_ptr(),
            FILE_SYSTEM::FAT32 => CStr::from_bytes_with_nul(b"fat32\0").unwrap().as_ptr(),
            FILE_SYSTEM::NONE => CStr::from_bytes_with_nul(b"none\0").unwrap().as_ptr(),
            FILE_SYSTEM::NTFS => CStr::from_bytes_with_nul(b"ntfs\0").unwrap().as_ptr(),
            FILE_SYSTEM::SWAP => CStr::from_bytes_with_nul(b"swap\0").unwrap().as_ptr(),
            FILE_SYSTEM::XFS => CStr::from_bytes_with_nul(b"xfs\0").unwrap().as_ptr(),
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn strfilesys(fs: FILE_SYSTEM) -> *const libc::c_char {
    fs.get_cstr()
}

#[repr(C)]
pub struct DistinstDisks {
    disks: *mut DistinstDisk,
    length: size_t,
}

impl Drop for DistinstDisks {
    fn drop(&mut self) {
        drop(unsafe { Vec::from_raw_parts(self.disks, self.length, self.length) });
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

/// The deconstructor for a `DistinstDisks`.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_destroy(disks: *mut DistinstDisks) {
    if !disks.is_null() {
        drop(Box::from_raw(disks))
    }
}

/// Attempts to obtain a specific partition's information based on it's index.
///
/// Returns a null pointer if the partition could not be found (index is out of bounds).
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_get(
    disks: *mut DistinstDisks,
    index: size_t,
) -> *mut DistinstDisk {
    if disks.is_null() {
        ptr::null_mut()
    } else if index >= (*disks).length {
        ptr::null_mut()
    } else {
        (*disks).disks.offset(index as isize)
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
    table_type: PARTITION_TABLE,
    read_only: uint8_t,
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
                None => PARTITION_TABLE::NONE,
                Some(PartitionTable::Msdos) => PARTITION_TABLE::MSDOS,
                Some(PartitionTable::Gpt) => PARTITION_TABLE::GPT,
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
                PARTITION_TABLE::GPT => Some(PartitionTable::Gpt),
                PARTITION_TABLE::MSDOS => Some(PartitionTable::Msdos),
                PARTITION_TABLE::NONE => None,
            },
            read_only: disk.read_only != 0,
            partitions: unsafe { Vec::from_raw_parts(parts, plen, plen) }
                .into_iter()
                .map(PartitionInfo::from)
                .collect::<Vec<_>>(),
        }
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

/// A destructor for a `DistinstDisk`
#[no_mangle]
pub unsafe extern "C" fn distinst_disk_destroy(disk: *mut DistinstDisk) {
    drop(Box::from_raw(disk))
}

/// Converts a `DistinstDisk` into a `Disk`, executes a given action with that `Disk`,
/// then converts it back into a `DistinstDisk`, returning the exit status of the function.
unsafe fn disk_action<F: Fn(&mut Disk) -> libc::c_int>(
    disk: *mut DistinstDisk,
    action: F,
) -> libc::c_int {
    let mut new_disk = Disk::from(*Box::from_raw(disk));
    let exit_status = action(&mut new_disk);
    *disk = DistinstDisk::from(new_disk);
    exit_status
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_add_partition(
    disk: *mut DistinstDisk,
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
    disk: *mut DistinstDisk,
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
    disk: *mut DistinstDisk,
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
    disk: *mut DistinstDisk,
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
    disk: *mut DistinstDisk,
    partition: libc::c_int,
    fs: FILE_SYSTEM,
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
pub unsafe extern "C" fn distinst_disk_commit(disk: *mut DistinstDisk) -> libc::c_int {
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
    filesystem: FILE_SYSTEM,
    part_type: PARTITION_TYPE,
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
        debug_assert!(distinst.filesystem != FILE_SYSTEM::NONE);

        PartitionBuilder {
            start_sector: distinst.start_sector as u64,
            end_sector: distinst.end_sector as u64,
            filesystem: Option::<FileSystemType>::from(distinst.filesystem).unwrap(),
            part_type: match distinst.part_type {
                PARTITION_TYPE::LOGICAL => PartitionType::Logical,
                PARTITION_TYPE::PRIMARY => PartitionType::Primary,
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
pub unsafe extern "C" fn distinst_disk_partition_builder_destroy(
    builder: *mut DistinstPartitionBuilder,
) {
    drop(Box::from_raw(builder));
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_partition_builder_new(
    start_sector: uint64_t,
    end_sector: uint64_t,
    filesystem: FILE_SYSTEM,
) -> *mut DistinstPartitionBuilder {
    let mut vec = Vec::with_capacity(8);
    let flags = vec.as_mut_ptr();
    let capacity = vec.capacity();
    mem::forget(vec);

    let builder = DistinstPartitionBuilder {
        start_sector,
        end_sector: end_sector - 1,
        filesystem,
        part_type: PARTITION_TYPE::PRIMARY,
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
pub unsafe extern "C" fn distinst_disk_partition_builder_set_name(
    builder: &mut DistinstPartitionBuilder,
    name: *mut libc::c_char,
) {
    (*builder).name = name;
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_partition_builder_set_mount(
    builder: &mut DistinstPartitionBuilder,
    target: *mut libc::c_char,
) {
    (*builder).target = target;
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_partition_builder_set_partition_type(
    builder: &mut DistinstPartitionBuilder,
    part_type: PARTITION_TYPE,
) {
    (*builder).part_type = part_type;
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_partition_builder_add_flag(
    builder: *mut DistinstPartitionBuilder,
    flag: PARTITION_FLAG,
) {
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
}

#[repr(C)]
pub struct DistinstPartition {
    is_source: uint8_t,
    remove: uint8_t,
    format: uint8_t,
    active: uint8_t,
    busy: uint8_t,
    part_type: PARTITION_TYPE,
    filesystem: FILE_SYSTEM,
    number: libc::int32_t,
    start_sector: uint64_t,
    end_sector: uint64_t,
    flags: DistinstPartitionFlags,
    name: *mut libc::c_char,
    device_path: *mut libc::c_char,
    mount_point: *mut libc::c_char,
    target: *mut libc::c_char,
}

impl From<PartitionInfo> for DistinstPartition {
    fn from(part: PartitionInfo) -> DistinstPartition {
        let mut pflags: Vec<PARTITION_FLAG> =
            part.flags.into_iter().map(PARTITION_FLAG::from).collect();
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
                PartitionType::Logical => PARTITION_TYPE::LOGICAL,
                PartitionType::Primary => PARTITION_TYPE::PRIMARY,
            },
            filesystem: part.filesystem.map_or(FILE_SYSTEM::NONE, FILE_SYSTEM::from),
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
                PARTITION_TYPE::LOGICAL => PartitionType::Logical,
                PARTITION_TYPE::PRIMARY => PartitionType::Primary,
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
    flags: *mut PARTITION_FLAG,
    length: size_t,
    capacity: size_t,
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
