use libc;

use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::ptr;

use distinst::{
    Disk, DiskExt, Disks, FileSystemType, LvmDevice, PartitionBuilder, PartitionInfo,
    PartitionTable, Sector,
};

use ffi::AsMutPtr;
use filesystem::DISTINST_FILE_SYSTEM_TYPE;
use gen_object_ptr;
use lvm::DistinstLvmDevice;
use partition::{DistinstPartition, DistinstPartitionBuilder, DISTINST_PARTITION_TABLE};
use sector::DistinstSector;

#[repr(C)]
pub struct DistinstDisk;

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
    match Disk::from_name(ostring) {
        Ok(disk) => gen_object_ptr(disk) as *mut DistinstDisk,
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
    drop(Box::from_raw(disk as *mut Disk))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_device_path(
    disk: *const DistinstDisk,
    len: *mut libc::c_int,
) -> *const u8 {
    let disk = &*(disk as *const Disk);
    let path = disk.get_device_path().as_os_str().as_bytes();
    *len = path.len() as libc::c_int;
    path.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_partition(
    disk: *mut DistinstDisk,
    partition: libc::int32_t,
) -> *mut DistinstPartition {
    let disk = &mut *(disk as *mut Disk);
    disk.get_partition_mut(partition as i32).as_mut_ptr() as *mut DistinstPartition
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_list_partitions(
    disk: *mut DistinstDisk,
    len: *mut libc::c_int,
) -> *mut *mut DistinstPartition {
    let disk = &mut *(disk as *mut Disk);

    let mut output: Vec<*mut DistinstPartition> = Vec::new();
    for partition in disk.get_partitions_mut().iter_mut() {
        output.push(partition as *mut PartitionInfo as *mut DistinstPartition);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *mut DistinstPartition
}

#[no_mangle]
/// TODO: This is to be used with vectors returned from
/// `distinst_disk_list_partitions`.
pub unsafe extern "C" fn distinst_disk_list_partitions_destroy(
    partitions: *mut DistinstPartition,
    len: libc::size_t,
) {
    drop(Vec::from_raw_parts(partitions, len, len))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_sectors(disk: *const DistinstDisk) -> libc::uint64_t {
    let disk = &*(disk as *const Disk);
    disk.get_sectors()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_sector_size(
    disk: *const DistinstDisk,
) -> libc::uint64_t {
    let disk = &*(disk as *const Disk);
    disk.get_sector_size()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_sector(
    disk: *const DistinstDisk,
    sector: *const DistinstSector,
) -> libc::uint64_t {
    let disk = &*(disk as *const Disk);
    disk.get_sector(Sector::from(*sector))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_mklabel(
    disk: *mut DistinstDisk,
    table: DISTINST_PARTITION_TABLE,
) -> libc::c_int {
    let disk = &mut *(disk as *mut Disk);

    let table = match table {
        DISTINST_PARTITION_TABLE::GPT => PartitionTable::Gpt,
        DISTINST_PARTITION_TABLE::MSDOS => PartitionTable::Msdos,
        _ => return -1,
    };

    if let Err(why) = disk.mklabel(table) {
        info!(
            "unable to write partition table on {}: {}",
            disk.path().display(),
            why
        );
        -1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_add_partition(
    disk: *mut DistinstDisk,
    partition: *mut DistinstPartitionBuilder,
) -> libc::c_int {
    let disk = &mut *(disk as *mut Disk);

    if let Err(why) = disk.add_partition(*Box::from_raw(partition as *mut PartitionBuilder)) {
        info!("unable to add partition: {}", why);
        -1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_remove_partition(
    disk: *mut DistinstDisk,
    partition: libc::c_int,
) -> libc::c_int {
    let disk = &mut *(disk as *mut Disk);

    if let Err(why) = disk.remove_partition(partition) {
        info!("unable to remove partition: {}", why);
        -1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_resize_partition(
    disk: *mut DistinstDisk,
    partition: libc::c_int,
    end: libc::uint64_t,
) -> libc::c_int {
    let disk = &mut *(disk as *mut Disk);

    if let Err(why) = disk.resize_partition(partition, end) {
        info!("libdistinst: unable to resize partition: {}", why);
        -1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_move_partition(
    disk: *mut DistinstDisk,
    partition: libc::c_int,
    start: libc::uint64_t,
) -> libc::c_int {
    let disk = &mut *(disk as *mut Disk);

    if let Err(why) = disk.move_partition(partition, start) {
        info!("unable to remove partition: {}", why);
        -1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_format_partition(
    disk: *mut DistinstDisk,
    partition: libc::c_int,
    fs: DISTINST_FILE_SYSTEM_TYPE,
) -> libc::c_int {
    let disk = &mut *(disk as *mut Disk);

    let fs = match Option::<FileSystemType>::from(fs) {
        Some(fs) => fs,
        None => {
            info!("file system type required");
            return -1;
        }
    };

    if let Err(why) = disk.format_partition(partition, fs.clone()) {
        info!("unable to remove partition: {}", why);
        -1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_commit(disk: *mut DistinstDisk) -> libc::c_int {
    let disk = &mut *(disk as *mut Disk);

    if let Err(why) = disk.commit() {
        info!("unable to commit changes to disk: {}", why);
        -1
    } else {
        0
    }
}

#[repr(C)]
pub struct DistinstDisks;

/// Returns an empty disks array
///
/// On error, a null pointer will be returned.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_new() -> *mut DistinstDisks {
    Box::into_raw(Box::new(Disks::new())) as *mut DistinstDisks
}

/// Probes the disk for information about every disk in the device.
///
/// On error, a null pointer will be returned.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_probe() -> *mut DistinstDisks {
    match Disks::probe_devices() {
        Ok(disks) => gen_object_ptr(disks) as *mut DistinstDisks,
        Err(why) => {
            info!("unable to probe devices: {}", why);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_list(
    disks: *mut DistinstDisks,
    len: *mut libc::c_int,
) -> *mut *mut DistinstDisk {
    let disks = &mut *(disks as *mut Disks);

    let mut output: Vec<*mut DistinstDisk> = Vec::new();
    for disk in disks.get_physical_devices_mut().iter_mut() {
        output.push(disk as *mut Disk as *mut DistinstDisk);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *mut DistinstDisk
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_list_logical(
    disks: *mut DistinstDisks,
    len: *mut libc::c_int,
) -> *mut *mut DistinstLvmDevice {
    let disks = &mut *(disks as *mut Disks);

    let mut output: Vec<*mut DistinstLvmDevice> = Vec::new();
    for disk in disks.get_logical_devices_mut().iter_mut() {
        output.push(disk as *mut LvmDevice as *mut DistinstLvmDevice);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *mut DistinstLvmDevice
}

#[no_mangle]
/// TODO: This is to be used with vectors returned from `distinst_disks_list`.
pub unsafe extern "C" fn distinst_disks_list_destroy(list: *mut DistinstDisk, len: libc::size_t) {
    drop(Vec::from_raw_parts(list, len, len))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_push(disks: *mut DistinstDisks, disk: *mut DistinstDisk) {
    (&mut *(disks as *mut Disks)).add(*Box::from_raw(disk as *mut Disk))
}

/// The deconstructor for a `DistinstDisks`.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_destroy(disks: *mut DistinstDisks) {
    drop(Box::from_raw(disks as *mut Disks))
}
