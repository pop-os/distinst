use distinst::{DiskExt, Disks, LvmDevice, PartitionInfo, Sector};
use ffi::AsMutPtr;
use libc;

use super::{get_str, DistinstDisks, DistinstPartition, DistinstPartitionBuilder, DistinstSector};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

c_methods!{
    use devices: DistinstDisks as Disks;

    mut fn distinst_disks_initialize_volume_groups() -> libc::c_int {
        match devices.initialize_volume_groups() {
            Ok(_) => 0,
            Err(why) => {
                error!("unable to initialize volumes: {}", why);
                -1
            }
        }
    } : -1

    mut fn distinst_disks_get_logical_device(
        string volume_group: *const libc::c_char
    ) -> *mut DistinstLvmDevice {
        devices.get_logical_device_mut(volume_group)
            .as_mut_ptr() as *mut DistinstLvmDevice
    } : ptr::null_mut()

    mut fn distinst_disks_get_logical_device_within_pv(
        string pv: *const libc::c_char
    ) -> *mut DistinstLvmDevice {
        devices.get_logical_device_within_pv_mut(pv)
            .as_mut_ptr() as *mut DistinstLvmDevice
    } : ptr::null_mut()
}

#[repr(C)]
pub struct DistinstLvmDevice;

c_methods!{
    use device: DistinstLvmDevice as LvmDevice;

    const fn distinst_lvm_device_get_model() -> *mut libc::c_char {
        CString::new(device.get_model()).ok()
            .map_or(ptr::null_mut(), |x| x.into_raw())
    } : ptr::null_mut()

    const fn distinst_lvm_device_last_used_sector() -> libc::uint64_t {
        device.get_partitions().iter().last()
            .map_or(0, |x| x.end_sector)
    } : 0

    const fn distinst_lvm_device_get_device_path(norm len: *mut libc::c_int) -> *const u8 {
        let path = device.get_device_path().as_os_str().as_bytes();
        *len = path.len() as libc::c_int;
        path.as_ptr()
    } : ptr::null()

    mut fn distinst_lvm_device_get_volume(
        string volume: *const libc::c_char
    ) -> *mut DistinstPartition {
        device.get_partition_mut(volume)
            .as_mut_ptr() as *mut DistinstPartition
    } : ptr::null_mut()

    mut fn distinst_lvm_device_get_partition_by_path(
        string path: *const libc::c_char
    ) -> *mut DistinstPartition {
        let path = Path::new(&path);
        device.get_partitions_mut()
            .iter_mut()
            .find(|d| d.get_device_path() == path)
            .as_mut_ptr() as *mut DistinstPartition
    } : ptr::null_mut()

    mut fn distinst_lvm_device_remove_partition(string mount: *const libc::c_char) -> bool {
        device.contains_mount(mount)
    } : false

    const fn distinst_lvm_device_get_sectors() -> libc::uint64_t {
        device.get_sectors()
    } : 0

    const fn distinst_lvm_device_get_sector_size() -> libc::uint64_t {
        device.get_sector_size()
    } : 0

    const fn distinst_lvm_device_get_sector(norm sector: *const DistinstSector) -> libc::uint64_t {
        device.get_sector(Sector::from(*sector))
    } : 0

    mut fn distinst_lvm_device_add_partition(
        boxed partition: *mut DistinstPartitionBuilder
    ) -> libc::c_int {
        if let Err(why) = device.add_partition(*partition) {
            error!("unable to add partition: {}", why);
            -1
        } else {
            0
        }
    } : -1

    mut fn distinst_lvm_device_list_partitions(
        norm len: *mut libc::c_int
    ) -> *mut *mut DistinstPartition {
        cvec_from!(
            for partition in device.get_partitions_mut().iter_mut(),
                push partition as *mut PartitionInfo as *mut DistinstPartition,
                record len
        )
    } : ptr::null_mut()

    const fn distinst_lvm_device_contains_mount(string mount: *const libc::c_char) -> bool {
        device.contains_mount(mount)
    } : false

    mut fn distinst_lvm_device_clear_partitions() -> () {
        device.clear_partitions();
    } : ()
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
