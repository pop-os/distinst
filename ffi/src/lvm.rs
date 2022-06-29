use distinst::{
    BlockDeviceExt, DiskExt, Disks, LogicalDevice, PartitionBuilder, PartitionInfo, Sector,
    SectorExt,
};
use external::luks::deactivate_logical_devices;
use crate::ffi::AsMutPtr;
use libc;

use super::{
    get_str, null_check, DistinstDisks, DistinstPartition, DistinstSector,
};
use std::{os::unix::ffi::OsStrExt, path::Path, ptr};

#[no_mangle]
pub unsafe extern "C" fn distinst_deactivate_logical_devices() -> libc::c_int {
    match deactivate_logical_devices() {
        Ok(()) => 0,
        Err(why) => {
            error!("unable to deactivate logical devices: {}", why);
            -1
        }
    }
}

// Initializes the initial volume groups within the disks object.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_initialize_volume_groups(
    disks: *mut DistinstDisks,
) -> libc::c_int {
    if null_check(disks).is_err() {
        return -1;
    }

    match (&mut *(disks as *mut Disks)).initialize_volume_groups() {
        Ok(_) => 0,
        Err(why) => {
            error!("unable to initialize volumes: {}", why);
            -1
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_get_logical_device(
    disks: *mut DistinstDisks,
    volume_group: *const libc::c_char,
) -> *mut DistinstLvmDevice {
    if null_check(disks).is_err() {
        return ptr::null_mut();
    }

    match get_str(volume_group) {
        Ok(vg) => {
            let disks = &mut *(disks as *mut Disks);
            info!("getting logical device named '{}'", vg);
            disks.get_logical_device_mut(vg).as_mut_ptr() as *mut DistinstLvmDevice
        }
        Err(why) => {
            eprintln!("libdistinst: volume_group is not UTF-8: {}", why);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_get_logical_device_within_pv(
    disks: *mut DistinstDisks,
    pv: *const libc::c_char,
) -> *mut DistinstLvmDevice {
    if null_check(disks).is_err() {
        return ptr::null_mut();
    }

    match get_str(pv) {
        Ok(pv) => {
            let disks = &mut *(disks as *mut Disks);
            info!("getting logical device");
            disks.get_logical_device_within_pv_mut(pv).as_mut_ptr() as *mut DistinstLvmDevice
        }
        Err(why) => {
            eprintln!("libdistinst: volume_group is not UTF-8: {}", why);
            ptr::null_mut()
        }
    }
}

#[repr(C)]
pub struct DistinstLvmDevice;

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_device_path(
    device: *const DistinstLvmDevice,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(device).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let device = &*(device as *const LogicalDevice);
    let path = device.get_device_path().as_os_str().as_bytes();
    *len = path.len() as libc::c_int;
    path.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_encrypted_file_system(
    device: *const DistinstLvmDevice,
) -> *const DistinstPartition {
    if null_check(device).is_err() {
        return ptr::null();
    }

    let device = &*(device as *const LogicalDevice);
    match device.get_file_system() {
        Some(fs) => fs as *const PartitionInfo as *const DistinstPartition,
        None => ptr::null(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_model(
    device: *mut DistinstLvmDevice,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(device).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let device = &mut *(device as *mut LogicalDevice);
    let model = device.get_model();
    *len = model.len() as libc::c_int;
    model.as_bytes().as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_last_used_sector(
    device: *const DistinstLvmDevice,
) -> u64 {
    if null_check(device).is_err() {
        return 0;
    }

    (&*(device as *const LogicalDevice)).get_partitions().iter().last().map_or(0, |p| p.end_sector)
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_sectors(
    device: *const DistinstLvmDevice,
) -> u64 {
    if null_check(device).is_err() {
        return 0;
    }

    let device = &*(device as *const LogicalDevice);
    device.get_sectors()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_sector_size(
    device: *const DistinstLvmDevice,
) -> u64 {
    if null_check(device).is_err() {
        return 0;
    }

    let device = &*(device as *const LogicalDevice);
    device.get_logical_block_size()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_sector(
    device: *const DistinstLvmDevice,
    sector: *const DistinstSector,
) -> u64 {
    if null_check(device).or_else(|_| null_check(sector)).is_err() {
        return 0;
    }

    (&*(device as *const LogicalDevice)).get_sector(Sector::from(*sector))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_volume(
    device: *mut DistinstLvmDevice,
    volume: *const libc::c_char,
) -> *mut DistinstPartition {
    if null_check(device).is_err() {
        return ptr::null_mut();
    }

    get_str(volume).ok().map_or(ptr::null_mut(), |volume| {
        let disk = &mut *(device as *mut LogicalDevice);
        disk.get_partition_mut(volume).as_mut_ptr() as *mut DistinstPartition
    })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_partition_by_path(
    device: *mut DistinstLvmDevice,
    path: *const libc::c_char,
) -> *mut DistinstPartition {
    if null_check(device).is_err() {
        return ptr::null_mut();
    }

    get_str(path)
        .ok()
        .and_then(|path| {
            let path = Path::new(&path);
            let device = &mut *(device as *mut LogicalDevice);
            device.get_partitions_mut().iter_mut().find(|d| d.get_device_path() == path)
        })
        .as_mut_ptr() as *mut DistinstPartition
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_list_partitions(
    device: *const DistinstLvmDevice,
    len: *mut libc::c_int,
) -> *mut *mut DistinstPartition {
    if null_check(device).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let device = &mut *(device as *mut LogicalDevice);

    let mut output: Vec<*mut DistinstPartition> = Vec::new();
    for partition in device.get_partitions_mut().iter_mut() {
        output.push(partition as *mut PartitionInfo as *mut DistinstPartition);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *mut DistinstPartition
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_contains_mount(
    device: *const DistinstLvmDevice,
    mount: *const libc::c_char,
    disks: *const DistinstDisks,
) -> bool {
    if null_check(device).or_else(|_| null_check(disks)).is_err() {
        return false;
    }

    get_str(mount).ok().map_or(false, |mount| {
        let device = &mut *(device as *mut LogicalDevice);
        let disks = &*(disks as *const Disks);
        device.contains_mount(mount, &*disks)
    })
}

#[repr(C)]
pub struct DistinstLuksEncryption {
    /// The PV field is not optional
    pub physical_volume: *mut libc::c_char,
    /// The password field is optional
    pub password:        *mut libc::c_char,
    /// The keydata field is optional
    pub keydata:         *mut libc::c_char,
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_encryption_copy(
    src: *const DistinstLuksEncryption,
    dst: *mut DistinstLuksEncryption,
) {
    if null_check(src).or_else(|_| null_check(dst)).is_err() {
        return;
    }

    let src = &*src;
    let dst = &mut *dst;

    dst.physical_volume = src.physical_volume;
    dst.password = src.password;
    dst.keydata = src.keydata;
}
