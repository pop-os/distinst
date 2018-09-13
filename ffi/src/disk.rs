use libc;

use std::ffi::{CStr, CString, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use distinst::{
    DecryptionError, Disk, DiskExt, Disks, FileSystemType, LvmDevice, LvmEncryption,
    PartitionBuilder, PartitionInfo, PartitionTable, Sector,
};

use super::{get_str, null_check};
use ffi::AsMutPtr;
use filesystem::DISTINST_FILE_SYSTEM_TYPE;
use gen_object_ptr;
use lvm::{DistinstLvmDevice, DistinstLvmEncryption};
use partition::{
    DistinstPartition, DistinstPartitionAndDiskPath, DistinstPartitionBuilder,
    DISTINST_PARTITION_TABLE,
};
use sector::DistinstSector;

#[repr(C)]
pub struct DistinstDisk;

/// Obtains a specific disk's information by the device path.
///
/// On an error, this will return a null pointer.
#[no_mangle]
pub unsafe extern "C" fn distinst_disk_new(path: *const libc::c_char) -> *mut DistinstDisk {
    if null_check(path).is_err() {
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
    if disk.is_null() {
        error!("DistinstDisk was to be destroyed even though it is null");
    } else {
        Box::from_raw(disk as *mut Disk);
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_device_path(
    disk: *const DistinstDisk,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(disk).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let disk = &*(disk as *const Disk);
    let path = disk.get_device_path().as_os_str().as_bytes();
    *len = path.len() as libc::c_int;
    path.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_model(
    disk: *mut DistinstDisk,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(disk).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let disk = &mut *(disk as *mut Disk);
    let model = disk.get_model();
    *len = model.len() as libc::c_int;
    model.as_bytes().as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_serial(
    disk: *mut DistinstDisk,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(disk).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let disk = &mut *(disk as *mut Disk);
    let serial = disk.get_serial();
    *len = serial.len() as libc::c_int;
    serial.as_bytes().as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_partition(
    disk: *mut DistinstDisk,
    partition: libc::int32_t,
) -> *mut DistinstPartition {
    if null_check(disk).is_err() {
        return ptr::null_mut();
    }

    let disk = &mut *(disk as *mut Disk);
    disk.get_partition_mut(partition as i32).as_mut_ptr() as *mut DistinstPartition
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_partition_by_path(
    disk: *mut DistinstDisk,
    path: *const libc::c_char,
) -> *mut DistinstPartition {
    if null_check(disk).is_err() {
        return ptr::null_mut();
    }

    get_str(path)
        .ok()
        .and_then(|path| {
            let path = Path::new(&path);
            let disk = &mut *(disk as *mut Disk);
            disk.get_partitions_mut()
                .iter_mut()
                .find(|d| d.get_device_path() == path)
        })
        .as_mut_ptr() as *mut DistinstPartition
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_contains_mount(
    disk: *const DistinstDisk,
    mount: *const libc::c_char,
    disks: *const DistinstDisks,
) -> bool {
    if null_check(disk).or_else(|_| null_check(disks)).is_err() {
        return false;
    }

    get_str(mount).ok().map_or(false, |mount| {
        let disk = &mut *(disk as *mut Disk);
        let disks = &*(disks as *const Disks);
        disk.contains_mount(mount, &*disks)
    })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_is_removable(disk: *mut DistinstDisk) -> bool {
    if null_check(disk).is_err() {
        return false;
    }

    let disk = &mut *(disk as *mut Disk);
    disk.is_removable()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_is_rotational(disk: *mut DistinstDisk) -> bool {
    if null_check(disk).is_err() {
        return false;
    }

    let disk = &mut *(disk as *mut Disk);
    disk.is_rotational()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_list_partitions(
    disk: *mut DistinstDisk,
    len: *mut libc::c_int,
) -> *mut *mut DistinstPartition {
    if null_check(disk).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

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
    if partitions.is_null() {
        error!("DistinstPartitions were to be destroyed but ")
    } else {
        Vec::from_raw_parts(partitions, len, len);
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_sectors(disk: *const DistinstDisk) -> libc::uint64_t {
    if null_check(disk).is_err() {
        return 0;
    }

    let disk = &*(disk as *const Disk);
    disk.get_sectors()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_sector_size(
    disk: *const DistinstDisk,
) -> libc::uint64_t {
    if null_check(disk).is_err() {
        return 0;
    }

    let disk = &*(disk as *const Disk);
    disk.get_sector_size()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_sector(
    disk: *const DistinstDisk,
    sector: *const DistinstSector,
) -> libc::uint64_t {
    if null_check(disk).or_else(|_| null_check(sector)).is_err() {
        return 0;
    }

    let disk = &*(disk as *const Disk);
    disk.get_sector(Sector::from(*sector))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_get_partition_table(
    disk: *const DistinstDisk
) -> DISTINST_PARTITION_TABLE {
    if null_check(disk).is_err() {
        return DISTINST_PARTITION_TABLE::NONE;
    }

    let disk = &*(disk as *const Disk);
    disk.get_table_type().into()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_mklabel(
    disk: *mut DistinstDisk,
    table: DISTINST_PARTITION_TABLE,
) -> libc::c_int {
    if null_check(disk).is_err() {
        return -1;
    }

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
    if null_check(disk).or_else(|_| null_check(partition)).is_err() {
        return -1;
    }

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
    if null_check(disk).is_err() {
        return -1;
    }

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
    if null_check(disk).is_err() {
        return 0;
    }

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
    if null_check(disk).is_err() {
        return -1;
    }

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
    if null_check(disk).is_err() {
        return -1;
    }

    let disk = &mut *(disk as *mut Disk);

    let fs = match Option::<FileSystemType>::from(fs) {
        Some(fs) => fs,
        None => {
            info!("file system type required");
            return -1;
        }
    };

    if let Err(why) = disk.format_partition(partition, fs) {
        info!("unable to remove partition: {}", why);
        -1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disk_commit(disk: *mut DistinstDisk) -> libc::c_int {
    if null_check(disk).is_err() {
        return -1;
    }

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
    Box::into_raw(Box::new(Disks::default())) as *mut DistinstDisks
}

/// A destructor for a `DistinstDisks`
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_destroy(disks: *mut DistinstDisks) {
    if disks.is_null() {
        error!("DistisntDisks was to be destroyed even though it is null");
    } else {
        Box::from_raw(disks as *mut Disks);
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_push(disks: *mut DistinstDisks, disk: *const DistinstDisk) {
    if null_check(disk).or_else(|_| null_check(disks)).is_err() {
        return;
    }

    let disks = &mut *(disks as *mut Disks);
    disks.add(ptr::read(disk as *const Disk));
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
pub unsafe extern "C" fn distinst_disks_contains_luks(
    disks: *const DistinstDisks
) -> bool {
    if null_check(disks).is_err() {
        return false;
    }

    let disks = &*(disks as *const Disks);
    disks.contains_luks()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_list(
    disks: *mut DistinstDisks,
    len: *mut libc::c_int,
) -> *mut *mut DistinstDisk {
    if null_check(disks).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let disks = &mut *(disks as *mut Disks);

    let mut output: Vec<*mut DistinstDisk> = Vec::new();
    for disk in disks.get_physical_devices_mut().iter_mut() {
        output.push(disk as *mut Disk as *mut DistinstDisk);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *mut DistinstDisk
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_get_physical_device(
    disks: *mut DistinstDisks,
    path: *const libc::c_char,
) -> *mut DistinstDisk {
    if null_check(disks).is_err() {
        return ptr::null_mut();
    }

    match get_str(path) {
        Ok(path) => {
            let disks = &mut *(disks as *mut Disks);
            disks.get_physical_device_mut(path).as_mut_ptr() as *mut DistinstDisk
        }
        Err(why) => {
            eprintln!("libdistinst: path is not UTF-8: {}", why);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_list_logical(
    disks: *mut DistinstDisks,
    len: *mut libc::c_int,
) -> *mut *mut DistinstLvmDevice {
    if null_check(disks).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let disks = &mut *(disks as *mut Disks);

    let mut output: Vec<*mut DistinstLvmDevice> = Vec::new();
    for disk in disks.get_logical_devices_mut().iter_mut() {
        output.push(disk as *mut LvmDevice as *mut DistinstLvmDevice);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *mut DistinstLvmDevice
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_find_partition(
    disks: *mut DistinstDisks,
    path: *const libc::c_char,
) -> *mut DistinstPartitionAndDiskPath {
    if null_check(disks).is_err() {
        return ptr::null_mut();
    }

    let disks = &mut *(disks as *mut Disks);
    let path = match get_str(path) {
        Ok(path) => path,
        Err(_) => {
            return ptr::null_mut();
        }
    };

    disks
        .find_partition_mut(Path::new(&path))
        .and_then(|(device_path, partition)| {
            CString::new(device_path.as_os_str().as_bytes())
                .ok()
                .map(|disk_path| {
                    let disk_path = disk_path.into_raw();
                    let partition = &mut *partition as *mut PartitionInfo as *mut DistinstPartition;
                    gen_object_ptr(DistinstPartitionAndDiskPath {
                        disk_path,
                        partition,
                    })
                })
        })
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_decrypt_partition(
    disks: *mut DistinstDisks,
    path: *const libc::c_char,
    enc: *mut DistinstLvmEncryption,
) -> libc::c_int {
    if null_check(disks)
        .or_else(|_| null_check(path))
        .or_else(|_| null_check(enc))
        .or_else(|_| null_check((*enc).physical_volume))
        .is_err()
    {
        return 1;
    }

    get_str(path).ok().map_or(2, |path| {
        get_str((*enc).physical_volume).ok().map_or(2, |pv| {
            let password = get_str((*enc).password).ok().map(String::from);
            let keydata = get_str((*enc).keydata).ok().map(String::from);
            if password.is_none() && keydata.is_none() {
                3
            } else {
                let enc = LvmEncryption::new(pv.into(), password, keydata);
                let disks = &mut *(disks as *mut Disks);
                match disks.decrypt_partition(&Path::new(path), &enc) {
                    Ok(_) => 0,
                    Err(why) => {
                        error!("decryption error: {}", why);
                        match why {
                            DecryptionError::Open { .. } => 4,
                            DecryptionError::DecryptedLacksVG { .. } => 5,
                            DecryptionError::LuksNotFound { .. } => 6,
                        }
                    }
                }
            }
        })
    })
}
