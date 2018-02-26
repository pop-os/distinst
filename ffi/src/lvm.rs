use distinst::{DiskExt, Disks, LvmDevice, PartitionBuilder, Sector};
use ffi::AsMutPtr;
use libc;

use super::{DistinstDisks, DistinstPartitionBuilder, DistinstSector};
use std::ffi::CStr;

// Initializes the initial volume groups within the disks object.
#[no_mangle]
pub unsafe extern "C" fn distinst_disks_initialize_volume_groups(disks: *mut DistinstDisks) {
    (&mut *(disks as *mut Disks)).initialize_volume_groups();
}

#[no_mangle]
pub unsafe extern "C" fn distinst_disks_find_logical_volume(
    disks: *mut DistinstDisks,
    group: *const libc::c_char,
) -> *mut DistinstLvmDevice {
    CStr::from_ptr(group)
        .to_str()
        .ok()
        .and_then(|group| (&mut *(disks as *mut Disks)).find_logical_disk_mut(group))
        .as_mut_ptr() as *mut DistinstLvmDevice
}

#[repr(C)]
pub struct DistinstLvmDevice;

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
pub unsafe extern "C" fn distinst_lvm_device_get_sector(
    device: *mut DistinstLvmDevice,
    sector: *const DistinstSector,
) -> libc::uint64_t {
    (&mut *(device as *mut LvmDevice)).get_sector(Sector::from(*sector))
}

#[no_mangle]
pub unsafe extern "C" fn distinst_lvm_device_add_partition(
    device: *mut DistinstLvmDevice,
    partition: *mut DistinstPartitionBuilder,
) -> libc::c_int {
    let disk = &mut *(device as *mut LvmDevice);

    if let Err(why) = disk.add_partition(*Box::from_raw(partition as *mut PartitionBuilder)) {
        info!("unable to add partition: {}", why);
        -1
    } else {
        0
    }
}
