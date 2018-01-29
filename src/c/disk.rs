use libc;

use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::ptr;

use {Disk, Disks, FileSystemType, PartitionBuilder, PartitionTable, Sector};
use c::filesystem::DISTINST_FILE_SYSTEM_TYPE;
use c::partition::{DistinstPartitionBuilder, DISTINST_PARTITION_TABLE};
use c::sector::DistinstSector;

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
        Ok(disk) => Box::into_raw(Box::new(disk)) as *mut DistinstDisk,
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

/// Converts a `DistinstDisk` into a `Disk`, executes a given action with that `Disk`,
/// then converts it back into a `DistinstDisk`, returning the exit status of the function.
unsafe fn disk_action<T, F: Fn(&Disk) -> T>(disk: *const DistinstDisk, action: F) -> T {
    action(if disk.is_null() {
        panic!("disk_action: disk is null")
    } else {
        &*(disk as *const Disk)
    })
}

/// Converts a `DistinstDisk` into a `Disk`, executes a given action with that `Disk`,
/// then converts it back into a `DistinstDisk`, returning the exit status of the function.
unsafe fn disk_action_mut<T, F: Fn(&mut Disk) -> T>(disk: *mut DistinstDisk, action: F) -> T {
    action(if disk.is_null() {
        panic!("disk_action: disk is null")
    } else {
        &mut *(disk as *mut Disk)
    })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_sector(
    disk: *const DistinstDisk,
    sector: *const DistinstSector,
) -> libc::uint64_t {
    disk_action(disk, |disk| disk.get_sector(Sector::from(*sector)))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_mklabel(
    disk: *mut DistinstDisk,
    table: DISTINST_PARTITION_TABLE,
) -> libc::c_int {
    let table = match table {
        DISTINST_PARTITION_TABLE::GPT => PartitionTable::Gpt,
        DISTINST_PARTITION_TABLE::MSDOS => PartitionTable::Msdos,
        _ => return 1,
    };

    disk_action_mut(disk, |disk| {
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

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_add_partition(
    disk: *mut DistinstDisk,
    partition: *mut DistinstPartitionBuilder,
) -> libc::c_int {
    disk_action_mut(disk, |disk| {
        if let Err(why) = disk.add_partition(*Box::from_raw(partition as *mut PartitionBuilder)) {
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
    disk_action_mut(disk, |disk| {
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
    length: libc::uint64_t,
) -> libc::c_int {
    disk_action_mut(disk, |disk| {
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
    start: libc::uint64_t,
) -> libc::c_int {
    disk_action_mut(disk, |disk| {
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
    fs: DISTINST_FILE_SYSTEM_TYPE,
) -> libc::c_int {
    let fs = match Option::<FileSystemType>::from(fs) {
        Some(fs) => fs,
        None => {
            info!("file system type required");
            return 1;
        }
    };

    disk_action_mut(disk, |disk| {
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
    disk_action_mut(disk, |disk| {
        if let Err(why) = disk.commit() {
            info!("unable to commit changes to disk: {}", why);
            1
        } else {
            0
        }
    })
}

#[repr(C)]
pub struct DistinstDisks;

/// Returns an empty disks array
///
/// On error, a null pointer will be returned.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_new() -> *mut DistinstDisks {
    Box::into_raw(Box::new(Disks(Vec::new()))) as *mut DistinstDisks
}

/// Probes the disk for information about every disk in the device.
///
/// On error, a null pointer will be returned.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_probe() -> *mut DistinstDisks {
    match Disks::probe_devices() {
        Ok(disks) => Box::into_raw(Box::new(disks)) as *mut DistinstDisks,
        Err(why) => {
            info!("unable to probe devices: {}", why);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_push(disks: *mut DistinstDisks, disk: *mut DistinstDisk) {
    (&mut *(disks as *mut Disks))
        .0
        .push(*Box::from_raw(disk as *mut Disk))
}

/// The deconstructor for a `DistinstDisks`.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_destroy(disks: *mut DistinstDisks) {
    drop(Box::from_raw(disks as *mut Disks))
}
