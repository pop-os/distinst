use libc;

use std::ffi::CStr;
use std::path::PathBuf;
use std::ptr;

use {Bootloader, PartitionBuilder, PartitionFlag, PartitionType};
use c::filesystem::DISTINST_FILE_SYSTEM_TYPE;

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

    Box::into_raw(Box::new(PartitionBuilder::new(
        start_sector,
        end_sector,
        filesystem,
    ))) as *mut DistinstPartitionBuilder
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
    println!("builder_action {:p}", builder);
    Box::into_raw(Box::new(action(if builder.is_null() {
        panic!("builder_action: builder is null")
    } else {
        *Box::from_raw(builder as *mut PartitionBuilder)
    }))) as *mut DistinstPartitionBuilder
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_name(
    builder: *mut DistinstPartitionBuilder,
    name: *mut libc::c_char,
) -> *mut DistinstPartitionBuilder {
    let name = match CStr::from_ptr(name).to_str() {
        Ok(string) => string.to_string(),
        Err(err) => {
            error!("distinst_partition_builder_name: invalid UTF-8: {}", err);
            return ptr::null_mut();
        }
    };

    builder_action(builder, move |builder| builder.name(name))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_builder_mount(
    builder: *mut DistinstPartitionBuilder,
    target: *mut libc::c_char,
) -> *mut DistinstPartitionBuilder {
    let target = match CStr::from_ptr(target).to_str() {
        Ok(string) => PathBuf::from(string.to_string()),
        Err(err) => {
            error!("distinst_partition_builder_mount: invalid UTF-8: {}", err);
            return ptr::null_mut();
        }
    };

    builder_action(builder, move |builder| builder.mount(target))
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

#[repr(C)]
pub struct DistinstPartition;
