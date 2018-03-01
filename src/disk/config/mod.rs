//! Contains source code related to the configuration of disks & partitions in the system.

mod disk;
mod disks;
mod disk_trait;
mod sector;
pub(crate) mod lvm;
pub(crate) mod partitions;

pub use self::disk::*;
pub use self::disks::*;
pub use self::disk_trait::DiskExt;
pub use self::lvm::{LvmDevice, LvmEncryption};
pub use self::partitions::*;
pub use self::sector::Sector;
pub(crate) use self::disk_trait::find_partition;

use super::{Bootloader, DiskError};
use libparted::{Device, Disk as PedDisk, DiskType as PedDiskType};
use std::ffi::OsString;
use std::fs::read_dir;
use std::path::Path;

/// Specifies whether the partition table on the disk is **MSDOS** or **GPT**.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionTable {
    Msdos,
    Gpt,
}

/// Obtains the UUID of the given device path by resolving symlinks in `/dev/disk/by-uuid`
/// until the device is found.
fn get_uuid(path: &Path) -> Option<OsString> {
    let uuid_dir = read_dir("/dev/disk/by-uuid").expect("unable to find /dev/disk/by-uuid");

    if let Ok(path) = path.canonicalize() {
        for uuid_entry in uuid_dir.filter_map(|entry| entry.ok()) {
            if &uuid_entry.path().canonicalize().unwrap() == &path {
                return Some(uuid_entry.file_name());
            }
        }
    }

    None
}

/// Gets a `libparted::Device` from the given name.
pub(crate) fn get_device<'a, P: AsRef<Path>>(name: P) -> Result<Device<'a>, DiskError> {
    info!("libdistinst: getting device at {}", name.as_ref().display());
    Device::get(name).map_err(|why| DiskError::DeviceGet { why })
}

/// Gets and opens a `libparted::Device` from the given name.
pub(crate) fn open_device<'a, P: AsRef<Path>>(name: P) -> Result<Device<'a>, DiskError> {
    info!("libdistinst: opening device at {}", name.as_ref().display());
    Device::new(name).map_err(|why| DiskError::DeviceGet { why })
}

/// Opens a `libparted::Disk` from a `libparted::Device`.
pub(crate) fn open_disk<'a>(device: &'a mut Device) -> Result<PedDisk<'a>, DiskError> {
    info!("libdistinst: opening disk at {}", device.path().display());
    let device = device as *mut Device;
    unsafe {
        match PedDisk::new(&mut *device) {
            Ok(disk) => Ok(disk),
            Err(_) => {
                info!("libdistinst: unable to open disk; creating new table on it");
                PedDisk::new_fresh(
                    &mut *device,
                    match Bootloader::detect() {
                        Bootloader::Bios => PedDiskType::get("msdos").unwrap(),
                        Bootloader::Efi => PedDiskType::get("gpt").unwrap(),
                    },
                ).map_err(|why| DiskError::DiskNew { why })
            }
        }
    }
}

/// Attempts to commit changes to the disk, return a `DiskError` on failure.
pub(crate) fn commit(disk: &mut PedDisk) -> Result<(), DiskError> {
    info!("libdistinst: commiting changes to {}", unsafe {
        disk.get_device().path().display()
    });
    disk.commit().map_err(|why| DiskError::DiskCommit { why })
}

/// Flushes the OS cache, return a `DiskError` on failure.
pub(crate) fn sync(device: &mut Device) -> Result<(), DiskError> {
    info!("libdistinst: syncing device at {}", device.path().display());
    device.sync().map_err(|why| DiskError::DiskSync { why })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_default() -> Disks {
        Disks {
            physical: vec![
                Disk {
                    mklabel:     false,
                    model_name:  "Test Disk".into(),
                    serial:      "Test Disk 123".into(),
                    device_path: "/dev/sdz".into(),
                    size:        1953525168,
                    sector_size: 512,
                    device_type: "TEST".into(),
                    table_type:  Some(PartitionTable::Gpt),
                    read_only:   false,
                    partitions:  vec![
                        PartitionInfo {
                            active:       true,
                            busy:         true,
                            is_source:    true,
                            remove:       false,
                            format:       false,
                            device_path:  Path::new("/dev/sdz1").to_path_buf(),
                            flags:        vec![],
                            mount_point:  Some(Path::new("/boot/efi").to_path_buf()),
                            target:       Some(Path::new("/boot/efi").to_path_buf()),
                            start_sector: 2048,
                            end_sector:   1026047,
                            filesystem:   Some(FileSystemType::Fat16),
                            name:         None,
                            number:       1,
                            part_type:    PartitionType::Primary,
                            swapped:      false,
                            key_id:       None,
                            volume_group: None,
                        },
                        PartitionInfo {
                            active:       true,
                            busy:         true,
                            is_source:    true,
                            remove:       false,
                            format:       false,
                            device_path:  Path::new("/dev/sdz2").to_path_buf(),
                            flags:        vec![],
                            mount_point:  Some(Path::new("/").to_path_buf()),
                            target:       Some(Path::new("/").to_path_buf()),
                            start_sector: 1026048,
                            end_sector:   420456447,
                            filesystem:   Some(FileSystemType::Btrfs),
                            name:         Some("Pop!_OS".into()),
                            number:       2,
                            part_type:    PartitionType::Primary,
                            swapped:      false,
                            key_id:       None,
                            volume_group: None,
                        },
                        PartitionInfo {
                            active:       false,
                            busy:         false,
                            is_source:    true,
                            remove:       false,
                            format:       false,
                            device_path:  Path::new("/dev/sdz3").to_path_buf(),
                            flags:        vec![],
                            mount_point:  None,
                            target:       None,
                            start_sector: 420456448,
                            end_sector:   1936738303,
                            filesystem:   Some(FileSystemType::Ext4),
                            name:         Some("Solus OS".into()),
                            number:       3,
                            part_type:    PartitionType::Primary,
                            swapped:      false,
                            key_id:       None,
                            volume_group: None,
                        },
                        PartitionInfo {
                            active:       true,
                            busy:         false,
                            is_source:    true,
                            remove:       false,
                            format:       false,
                            device_path:  Path::new("/dev/sdz4").to_path_buf(),
                            flags:        vec![],
                            mount_point:  None,
                            target:       None,
                            start_sector: 1936738304,
                            end_sector:   1953523711,
                            filesystem:   Some(FileSystemType::Swap),
                            name:         None,
                            number:       4,
                            part_type:    PartitionType::Primary,
                            swapped:      false,
                            key_id:       None,
                            volume_group: None,
                        },
                    ],
                },
            ],
            logical:  Vec::new(),
        }
    }

    fn get_empty() -> Disks {
        Disks {
            physical: vec![
                Disk {
                    mklabel:     false,
                    model_name:  "Test Disk".into(),
                    serial:      "Test Disk 123".into(),
                    device_path: "/dev/sdz".into(),
                    size:        1953525168,
                    sector_size: 512,
                    device_type: "TEST".into(),
                    table_type:  Some(PartitionTable::Gpt),
                    read_only:   false,
                    partitions:  Vec::new(),
                },
            ],
            logical:  Vec::new(),
        }
    }

    const GIB20: u64 = 41943040;

    // 500 MiB Fat16 partition.
    fn boot_part(start: u64) -> PartitionBuilder {
        PartitionBuilder::new(start, 1024_000 + start, FileSystemType::Fat16)
    }

    // 20 GiB Ext4 partition.
    fn root_part(start: u64) -> PartitionBuilder {
        PartitionBuilder::new(start, GIB20 + start, FileSystemType::Ext4)
    }

    #[test]
    fn layout_diff() {
        let source = get_default().physical.into_iter().next().unwrap();
        let mut new = source.clone();
        new.remove_partition(1).unwrap();
        new.remove_partition(2).unwrap();
        new.format_partition(3, FileSystemType::Xfs).unwrap();
        let start = new.get_partition(3).unwrap().start_sector;
        new.resize_partition(3, start + GIB20).unwrap();
        new.remove_partition(4).unwrap();
        new.add_partition(boot_part(2048)).unwrap();
        new.add_partition(root_part(1026_048)).unwrap();
        assert_eq!(
            source.diff(&new).unwrap(),
            DiskOps {
                mklabel:           None,
                device_path:       Path::new("/dev/sdz"),
                remove_partitions: vec![1, 2, 3, 4],
                change_partitions: vec![],
                create_partitions: vec![
                    PartitionCreate {
                        start_sector: 420456448,
                        end_sector:   420456447 + GIB20,
                        file_system:  Some(FileSystemType::Xfs),
                        kind:         PartitionType::Primary,
                        flags:        vec![],
                        format:       true,
                        label:        None,
                        path:         PathBuf::from("/dev/sdz"),
                    },
                    PartitionCreate {
                        start_sector: 2048,
                        end_sector:   1024_000 + 2047,
                        file_system:  Some(FileSystemType::Fat16),
                        kind:         PartitionType::Primary,
                        flags:        vec![],
                        format:       true,
                        label:        None,
                        path:         PathBuf::from("/dev/sdz"),
                    },
                    PartitionCreate {
                        start_sector: 1026_048,
                        end_sector:   GIB20 + 1026_047,
                        file_system:  Some(FileSystemType::Ext4),
                        kind:         PartitionType::Primary,
                        flags:        vec![],
                        format:       true,
                        label:        None,
                        path:         PathBuf::from("/dev/sdz"),
                    },
                ],
            }
        )
    }

    #[test]
    fn partition_add() {
        // The default sample is maxed out, so any partition added should fail.
        let mut source = get_default().physical.into_iter().next().unwrap();
        assert!(
            source
                .add_partition(PartitionBuilder::new(2048, 2_000_000, FileSystemType::Ext4))
                .is_err()
        );

        // Failures should also occur if the end sector exceeds the size of
        assert!(
            source
                .add_partition(PartitionBuilder::new(
                    2048,
                    1953525169,
                    FileSystemType::Ext4
                ))
                .is_err()
        );

        // An empty disk should succeed, on the other hand.
        let mut source = get_empty().physical.into_iter().next().unwrap();

        // Create 500MiB Fat16 partition w/ 512 byte sectors.
        source.add_partition(boot_part(2048)).unwrap();

        // This should fail with an off by one error, due to the start
        // sector being located within the previous partition.
        assert!(source.add_partition(root_part(1026_047)).is_err());

        // Create 20GiB Ext4 partition after that.
        source.add_partition(root_part(1026_048)).unwrap();
    }

    #[test]
    fn layout_validity() {
        // This test ensures that invalid layouts will raise a flag. An invalid layout is
        // a layout which is missing some of the original source partitions.
        let source = get_default().physical.into_iter().next().unwrap();
        let mut duplicate = source.clone();
        assert!(source.validate_layout(&duplicate).is_ok());

        // This should fail, because a critical source partition was removed.
        duplicate.partitions.remove(0);
        assert!(source.validate_layout(&duplicate).is_err());

        // An empty partition should always succeed.
        let source = get_empty().physical.into_iter().next().unwrap();
        let mut duplicate = source.clone();
        assert!(source.validate_layout(&duplicate).is_ok());
        duplicate
            .add_partition(PartitionBuilder::new(
                2048,
                1024_000 + 2048,
                FileSystemType::Fat16,
            ))
            .unwrap();
        assert!(source.validate_layout(&duplicate).is_ok());
    }
}
