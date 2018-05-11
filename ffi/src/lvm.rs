use distinst::{DiskExt, Disks, LvmDevice, PartitionBuilder, PartitionInfo, Sector};
use ffi::AsMutPtr;
use libc;

use super::{get_str, DistinstDisks, DistinstPartition, DistinstPartitionBuilder, DistinstSector};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

// Initializes the initial volume groups within the disks object.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_initialize_volume_groups(
    disks: *mut DistinstDisks,
) -> libc::c_int {
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
    match get_str(volume_group, "distinst_disks_get_logical_device") {
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
    match get_str(pv, "distinst_disks_get_logical_device_within_pv") {
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
    disk: *const DistinstLvmDevice,
    len: *mut libc::c_int,
) -> *const u8 {
    let disk = &*(disk as *const LvmDevice);
    let path = disk.get_device_path().as_os_str().as_bytes();
    *len = path.len() as libc::c_int;
    path.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_model(
    disk: *mut DistinstLvmDevice,
) -> *mut libc::c_char {
    let disk = &mut *(disk as *mut LvmDevice);
    CString::new(disk.get_model())
        .ok()
        .map(|string| string.into_raw())
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_last_used_sector(
    device: *mut DistinstLvmDevice,
) -> libc::uint64_t {
    (&mut *(device as *mut LvmDevice))
        .get_partitions()
        .iter()
        .last()
        .map_or(0, |p| p.end_sector)
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_sectors(
    disk: *const DistinstLvmDevice,
) -> libc::uint64_t {
    let disk = &*(disk as *const LvmDevice);
    disk.get_sectors()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_sector_size(
    disk: *const DistinstLvmDevice,
) -> libc::uint64_t {
    let disk = &*(disk as *const LvmDevice);
    disk.get_sector_size()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_sector(
    device: *mut DistinstLvmDevice,
    sector: *const DistinstSector,
) -> libc::uint64_t {
    (&mut *(device as *mut LvmDevice)).get_sector(Sector::from(*sector))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_volume(
    device: *mut DistinstLvmDevice,
    volume: *const libc::c_char,
) -> *mut DistinstPartition {
    get_str(volume, "distinst_lvm_device_get_volume")
        .ok()
        .map_or(ptr::null_mut(), |volume| {
            let disk = &mut *(device as *mut LvmDevice);
            disk.get_partition_mut(volume).as_mut_ptr() as *mut DistinstPartition
        })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_get_partition_by_path(
    disk: *mut DistinstLvmDevice,
    path: *const libc::c_char,
) -> *mut DistinstPartition {
    get_str(path, "")
        .ok()
        .and_then(|path| {
            let path = Path::new(&path);
            let disk = &mut *(disk as *mut LvmDevice);
            disk.get_partitions_mut()
                .iter_mut()
                .find(|d| d.get_device_path() == path)
        })
        .as_mut_ptr() as *mut DistinstPartition
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_add_partition(
    device: *mut DistinstLvmDevice,
    partition: *mut DistinstPartitionBuilder,
) -> libc::c_int {
    let disk = &mut *(device as *mut LvmDevice);

    if let Err(why) = disk.add_partition(*Box::from_raw(partition as *mut PartitionBuilder)) {
        error!("unable to add partition: {}", why);
        -1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_remove_partition(
    device: *mut DistinstLvmDevice,
    volume: *const libc::c_char,
) -> libc::c_int {
    get_str(volume, "distinst_lvm_device_remove_partition")
        .ok()
        .map_or(1, |volume| {
            let disk = &mut *(device as *mut LvmDevice);
            disk.remove_partition(volume).ok().map_or(2, |_| 0)
        })
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_clear_partitions(device: *mut DistinstLvmDevice) {
    let disk = &mut *(device as *mut LvmDevice);
    disk.clear_partitions();
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_list_partitions(
    device: *const DistinstLvmDevice,
    len: *mut libc::c_int,
) -> *mut *mut DistinstPartition {
    let disk = &mut *(device as *mut LvmDevice);

    let mut output: Vec<*mut DistinstPartition> = Vec::new();
    for partition in disk.get_partitions_mut().iter_mut() {
        output.push(partition as *mut PartitionInfo as *mut DistinstPartition);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *mut DistinstPartition
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_contains_mount(
    disk: *const DistinstLvmDevice,
    mount: *const libc::c_char,
) -> bool {
    get_str(mount, "").ok().map_or(false, |mount| {
        let disk = &mut *(disk as *mut LvmDevice);
        disk.contains_mount(mount)
    })
}

#[repr(C)]
pub struct DistinstLvmEncryption {
    /// The PV field is not optional
    pub physical_volume: *mut libc::c_char,
    /// The password field is optional
    pub password: *mut libc::c_char,
    /// The keydata field is optional
    pub keydata: *mut libc::c_char,
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_encryption_copy(
    src: *const DistinstLvmEncryption,
    dst: *mut DistinstLvmEncryption,
) {
    let src = &*src;
    let dst = &mut *dst;

    dst.physical_volume = src.physical_volume;
    dst.password = src.password;
    dst.keydata = src.keydata;
}
