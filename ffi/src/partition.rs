use libc;

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;

use {gen_object_ptr, get_str, DistinstLvmEncryption};
use distinst::{Bootloader, LvmEncryption, PartitionBuilder, PartitionFlag, PartitionInfo, PartitionType};
use filesystem::DISTINST_FILE_SYSTEM_TYPE;

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

impl From<PartitionType> for DISTINST_PARTITION_TYPE {
    fn from(part_type: PartitionType) -> DISTINST_PARTITION_TYPE {
        match part_type {
            PartitionType::Primary => DISTINST_PARTITION_TYPE::PRIMARY,
            PartitionType::Logical => DISTINST_PARTITION_TYPE::LOGICAL,
        }
    }
}

impl From<DISTINST_PARTITION_TYPE> for PartitionType {
    fn from(part_type: DISTINST_PARTITION_TYPE) -> PartitionType {
        match part_type {
            DISTINST_PARTITION_TYPE::PRIMARY => PartitionType::Primary,
            DISTINST_PARTITION_TYPE::LOGICAL => PartitionType::Logical,
        }
    }
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
pub struct DistinstPartitionBuilder;

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_new(
    start_sector: libc::uint64_t,
    end_sector: libc::uint64_t,
    filesystem: DISTINST_FILE_SYSTEM_TYPE,
) -> *mut DistinstPartitionBuilder {
    let filesystem = match filesystem.into() {
        Some(filesystem) => filesystem,
        None => {
            error!("distinst_partition_builder_new: filesystem is NONE");
            return ptr::null_mut();
        }
    };

    gen_object_ptr(PartitionBuilder::new(start_sector, end_sector, filesystem))
        as *mut DistinstPartitionBuilder
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_destroy(
    builder: *mut DistinstPartitionBuilder,
) {
    drop(Box::from_raw(builder as *mut PartitionBuilder));
}

/// Converts a `DistinstPartitionBuilder` into a `PartitionBuilder`, executes a given action with
/// that `PartitionBuilder`, then converts it back into a `DistinstPartitionBuilder`, returning the
/// exit status of the function.
unsafe fn builder_action<F: FnOnce(PartitionBuilder) -> PartitionBuilder>(
    builder: *mut DistinstPartitionBuilder,
    action: F,
) -> *mut DistinstPartitionBuilder {
    gen_object_ptr(action(if builder.is_null() {
        panic!("builder_action: builder is null")
    } else {
        *Box::from_raw(builder as *mut PartitionBuilder)
    })) as *mut DistinstPartitionBuilder
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_name(
    builder: *mut DistinstPartitionBuilder,
    name: *const libc::c_char,
) -> *mut DistinstPartitionBuilder {
    let name = match get_str(name, "distinst_partition_builder") {
        Ok(string) => string.to_string(),
        Err(why) => panic!("builder_action: failed: {}", why),
    };

    builder_action(builder, move |builder| builder.name(name))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_mount(
    builder: *mut DistinstPartitionBuilder,
    target: *const libc::c_char,
) -> *mut DistinstPartitionBuilder {
    let target = match get_str(target, "distinst_partition_builder_mount") {
        Ok(string) => PathBuf::from(string.to_string()),
        Err(why) => panic!("builder_action: failed: {}", why),
    };

    builder_action(builder, move |builder| builder.mount(target))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_associate_keyfile(
    builder: *mut DistinstPartitionBuilder,
    keyid: *const libc::c_char,
) -> *mut DistinstPartitionBuilder {
    let keyid = match get_str(keyid, "distinst_partition_builder_keyfile") {
        Ok(string) => string.to_string(),
        Err(why) => panic!("builder_action: failed: {}", why),
    };

    builder_action(builder, move |builder| builder.associate_keyfile(keyid))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_partition_type(
    builder: *mut DistinstPartitionBuilder,
    part_type: DISTINST_PARTITION_TYPE,
) -> *mut DistinstPartitionBuilder {
    builder_action(builder, |builder| builder.partition_type(part_type.into()))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_flag(
    builder: *mut DistinstPartitionBuilder,
    flag: DISTINST_PARTITION_FLAG,
) -> *mut DistinstPartitionBuilder {
    builder_action(builder, |builder| builder.flag(flag.into()))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_logical_volume(
    builder: *mut DistinstPartitionBuilder,
    group: *const libc::c_char,
    encryption: *mut DistinstLvmEncryption,
) -> *mut DistinstPartitionBuilder {
    let group = match get_str(group, "distinst_partition_builder_logical_volume") {
        Ok(string) => String::from(string.to_string()),
        Err(why) => panic!("builder_action: failed: {}", why),
    };

    let encryption = if encryption.is_null() {
        None
    } else {
        let pv = match get_str(
            (*encryption).physical_volume,
            "distinst_partition_builder_logical_volume",
        ) {
            Ok(string) => String::from(string.to_string()),
            Err(why) => panic!("builder_action: failed: {}", why),
        };

        let password = if (*encryption).password.is_null() {
            None
        } else {
            match get_str(
                (*encryption).password,
                "distinst_partition_builder_logical_volume",
            ) {
                Ok(string) => Some(String::from(string.to_string())),
                Err(why) => panic!("builder_action: failed: {}", why),
            }
        };

        let keydata = if (*encryption).keydata.is_null() {
            None
        } else {
            match get_str(
                (*encryption).keydata,
                "distinst_partition_builder_logical_volume",
            ) {
                Ok(string) => Some(String::from(string.to_string())),
                Err(why) => panic!("builder_action: failed: {}", why),
            }
        };

        Some(LvmEncryption::new(pv, password, keydata))
    };

    builder_action(builder, |builder| builder.logical_volume(group, encryption))
}

#[repr(C)]
pub struct DistinstPartition;

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_get_device_path(
    partition: *const DistinstPartition,
    len: *mut libc::c_int,
) -> *const u8 {
    let part = &*(partition as *const PartitionInfo);
    let path = part.get_device_path().as_os_str().as_bytes();
    *len = path.len() as libc::c_int;
    path.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_get_file_system(
    partition: *const DistinstPartition,
) -> DISTINST_FILE_SYSTEM_TYPE {
    let part = &*(partition as *const PartitionInfo);
    match part.filesystem {
        Some(fs) => DISTINST_FILE_SYSTEM_TYPE::from(fs),
        None => DISTINST_FILE_SYSTEM_TYPE::NONE,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_get_label(
    partition: *const DistinstPartition,
) -> *mut libc::c_char {
    let part = &*(partition as *const PartitionInfo);
    part.name
        .clone()
        .and_then(|osstr| CString::new(osstr).ok().map(|string| string.into_raw()))
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_get_start_sector(
    partition: *const DistinstPartition,
) -> libc::uint64_t {
    let part = &*(partition as *const PartitionInfo);
    part.start_sector
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_get_end_sector(
    partition: *const DistinstPartition,
) -> libc::uint64_t {
    let part = &*(partition as *const PartitionInfo);
    part.end_sector
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_set_mount(
    partition: *mut DistinstPartition,
    target: *const libc::c_char,
) {
    let target = match get_str(target, "distinst_partition_set_mount") {
        Ok(string) => PathBuf::from(string.to_string()),
        Err(why) => panic!("partition action: failed: {}", why),
    };

    let part = &mut *(partition as *mut PartitionInfo);
    part.set_mount(target);
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_associate_keyfile(
    partition: *mut DistinstPartition,
    keyid: *const libc::c_char,
) {
    let keyid = match get_str(keyid, "distinst_partition_associate_keyfile") {
        Ok(string) => string.to_string(),
        Err(why) => panic!("partition action: failed: {}", why),
    };

    let part = &mut *(partition as *mut PartitionInfo);
    part.associate_keyfile(keyid);
}


#[no_mangle]
pub unsafe extern "C" fn distinst_partition_set_flags(
    partition: *mut DistinstPartition,
    ptr: *const DISTINST_PARTITION_FLAG,
    len: libc::size_t,
) {
    let targets = ::std::slice::from_raw_parts(ptr, len as usize)
        .iter()
        .map(|flag| PartitionFlag::from(*flag))
        .collect::<Vec<PartitionFlag>>();

    let part = &mut *(partition as *mut PartitionInfo);
    part.flags = targets;
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_format_with(
    partition: *mut DistinstPartition,
    fs: DISTINST_FILE_SYSTEM_TYPE,
) -> libc::c_int {
    let part = &mut *(partition as *mut PartitionInfo);
    part.format_with(match fs.into() {
        Some(fs) => fs,
        None => return -1,
    });
    0
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_probe_os(
    partition: *const DistinstPartition,
) -> *mut libc::c_char {
    let part = &*(partition as *const PartitionInfo);
    part.probe_os()
        .and_then(|osstr| CString::new(osstr).ok().map(|string| string.into_raw()))
        .unwrap_or(ptr::null_mut())
}

#[repr(C)]
pub struct DistinstPartitionUsage {
    // 0 = None, 1 = Some(Ok(T)), 2 = Some(Err(T))
    tag: libc::uint8_t,
    // Some(Ok(sectors)) | Some(Err(errno))
    value: libc::uint64_t,
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_sectors_used(
    partition: *const DistinstPartition,
    sector_size: libc::uint64_t,
) -> DistinstPartitionUsage {
    let part = &*(partition as *const PartitionInfo);
    match part.sectors_used(sector_size) {
        None => DistinstPartitionUsage { tag:   0, value: 0 },
        Some(Ok(used)) => DistinstPartitionUsage {
            tag:   1,
            value: used,
        },
        Some(Err(why)) => {
            error!("unable to get partition sector usage: {}", why);
            DistinstPartitionUsage { tag:   2, value: 0 }
        }
    }
}
