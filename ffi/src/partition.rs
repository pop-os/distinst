use libc;

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr;

use distinst::{
    LvmEncryption, PartitionBuilder, PartitionFlag, PartitionInfo
};
use filesystem::DISTINST_FILE_SYSTEM_TYPE;
use super::*;

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

/// Converts a `DistinstPartitionBuilder` into a `PartitionBuilder`, executes a given action with
/// that `PartitionBuilder`, then converts it back into a `DistinstPartitionBuilder`, returning the
/// exit status of the function.
unsafe fn builder_action<F: FnOnce(PartitionBuilder) -> PartitionBuilder>(
    builder: *mut PartitionBuilder,
    action: F,
) -> *mut DistinstPartitionBuilder {
    gen_object_ptr(action(*Box::from_raw(builder)))
        as *mut DistinstPartitionBuilder
}

c_methods!{
    use builder: DistinstPartitionBuilder as PartitionBuilder;

    mut fn distinst_partition_builder_destroy() -> () {
        drop(Box::from_raw(builder));
    } : ()

    mut fn distinst_partition_builder_name(
        string name: *const libc::c_char
    ) -> *mut DistinstPartitionBuilder {
        builder_action(builder, move |builder| builder.name(name.into()))
    } : ptr::null_mut()

    mut fn distinst_partition_builder_mount(
        string target: *const libc::c_char
    ) -> *mut DistinstPartitionBuilder {
        builder_action(builder, move |builder| builder.mount(PathBuf::from(target.to_string())))
    } : ptr::null_mut()

    mut fn distinst_partition_builder_associate_keyfile(
        string keyid: *const libc::c_char
    ) -> *mut DistinstPartitionBuilder {
        builder_action(builder, move |builder| builder.associate_keyfile(keyid.into()))
    } : ptr::null_mut()

    mut fn distinst_partition_builder_partition_type(
        norm part_type: DISTINST_PARTITION_TYPE
    ) -> *mut DistinstPartitionBuilder {
        builder_action(builder, move |builder| builder.partition_type(part_type.into()))
    } : ptr::null_mut()

    mut fn distinst_partition_builder_flag(
        norm flag: DISTINST_PARTITION_FLAG
    ) -> *mut DistinstPartitionBuilder {
        builder_action(builder, move |builder| builder.flag(flag.into()))
    } : ptr::null_mut()

    mut fn distinst_partition_builder_logical_volume(
        string group: *const libc::c_char,
        norm encryption: *mut DistinstLvmEncryption
    ) -> *mut DistinstPartitionBuilder {
        let group = group.into();

        let encryption = if encryption.is_null() {
            None
        } else {
            // TODO: Make an abstraction for these.
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
    } : ptr::null_mut()
}

#[repr(C)]
pub struct DistinstPartition;

c_methods!{
    use part: DistinstPartition as PartitionInfo;

    const fn distinst_partition_get_current_lvm_volume_group() -> *mut libc::c_char {
        part.get_current_lvm_volume_group()
            .clone()
            .and_then(|osstr| CString::new(osstr).ok().map(|string| string.into_raw()))
            .unwrap_or(ptr::null_mut())
    } : ptr::null_mut()

    const fn distinst_partition_get_number() -> libc::int32_t {
        part.number
    } : -1

    const fn distinst_partition_get_device_path(norm len: *mut libc::c_int) -> *const u8 {
        let path = part.get_device_path().as_os_str().as_bytes();
        *len = path.len() as libc::c_int;
        path.as_ptr()
    } : ptr::null_mut()

    const fn distinst_partition_get_file_system() -> DISTINST_FILE_SYSTEM_TYPE {
        match part.filesystem {
            Some(fs) => DISTINST_FILE_SYSTEM_TYPE::from(fs),
            None => DISTINST_FILE_SYSTEM_TYPE::NONE,
        }
    } : DISTINST_FILE_SYSTEM_TYPE::NONE

    const fn distinst_partition_get_label() -> *mut libc::c_char {
        part.name
            .clone()
            .and_then(|osstr| CString::new(osstr).ok().map(|string| string.into_raw()))
            .unwrap_or(ptr::null_mut())
    } : ptr::null_mut()

    const fn distinst_partition_get_mount_point() -> *mut libc::c_char {
        part.mount_point
            .clone()
            .and_then(|path| {
                CString::new(path.as_os_str().as_bytes())
                    .ok()
                    .map(|string| string.into_raw())
            })
            .unwrap_or(ptr::null_mut())
    } : ptr::null_mut()

    const fn distinst_partition_get_start_sector() -> libc::uint64_t {
        part.start_sector
    } : 0

    const fn distinst_partition_get_end_sector() -> libc::uint64_t {
        part.end_sector
    } : 0

    mut fn distinst_partition_set_mount(string target: *const libc::c_char) -> () {
        part.set_mount(PathBuf::from(target.to_string()));
    } : ()

    mut fn distinst_partition_associate_keyfile(string keyid: *const libc::c_char) -> () {
        part.associate_keyfile(keyid.to_string());
    } : ()

    mut fn distinst_partition_set_flags(
        norm ptr: *const DISTINST_PARTITION_FLAG,
        norm len: libc::size_t
    ) -> () {
        part.flags = ::std::slice::from_raw_parts(ptr, len as usize)
            .iter()
            .map(|flag| PartitionFlag::from(*flag))
            .collect::<Vec<PartitionFlag>>();
    } : ()

    mut fn distinst_partition_format_and_keep_name(
        norm fs: DISTINST_FILE_SYSTEM_TYPE
    ) -> libc::c_int {
        part.format_and_keep_name(match fs.into() {
            Some(fs) => fs,
            None => return -1,
        });
        0
    } : -1

    mut fn distinst_partition_format_with(norm fs: DISTINST_FILE_SYSTEM_TYPE) -> libc::c_int {
        part.format_with(match fs.into() {
            Some(fs) => fs,
            None => return -1,
        });
        0
    } : -1
}

#[repr(C)]
pub struct DistinstOsInfo {
    os:   *mut libc::c_char,
    home: *mut libc::c_char,
}

impl Default for DistinstOsInfo {
    fn default() -> DistinstOsInfo {
        DistinstOsInfo {
            os:   ptr::null_mut(),
            home: ptr::null_mut(),
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_probe_os(
    partition: *const DistinstPartition,
) -> DistinstOsInfo {
    let part = &*(partition as *const PartitionInfo);

    let (os_str, home_path) = match part.probe_os() {
        Some(info) => info,
        None => {
            return DistinstOsInfo::default();
        }
    };

    DistinstOsInfo {
        os:   CString::new(os_str)
            .ok()
            .map(|string| string.into_raw())
            .unwrap_or(ptr::null_mut()),
        home: home_path
            .and_then(|path| CString::new(path.into_os_string().into_vec()).ok())
            .map(|string| string.into_raw())
            .unwrap_or(ptr::null_mut()),
    }
}

#[repr(C)]
pub struct DistinstPartitionAndDiskPath {
    pub disk_path: *mut libc::c_char,
    pub partition: *mut DistinstPartition,
}

#[no_mangle]
pub unsafe extern "C" fn distinst_partition_and_disk_path_destroy(
    object: *mut DistinstPartitionAndDiskPath,
) {
    CString::from_raw(Box::from_raw(object).disk_path);
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
