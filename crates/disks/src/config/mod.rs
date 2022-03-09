//! Contains source code related to the configuration of disks & partitions in
//! the system.

mod disk;
mod disk_trait;
mod disks;
mod lvm;
mod partitions;

pub use self::{
    disk::*,
    disk_trait::{find_partition, find_partition_mut, DiskExt},
    disks::*,
    lvm::*,
    partitions::*,
};
pub use disk_types::{PartitionTable, Sector};

use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};
use sysfs_class::{Block, SysClass};

static mut PVS: Option<BTreeMap<PathBuf, Option<String>>> = None;

/// Obtains the size of the device, in bytes, from a given block device.
/// Note: This is only to be used with getting partition sizes of logical volumes.
pub fn get_size(path: &Path) -> io::Result<u64> {
    let name: String = match path.canonicalize() {
        Ok(path) => {
            path.file_name().expect("device does not have a file name").to_str().unwrap().into()
        }
        Err(_) => {
            path.file_name().expect("device does not have a file name").to_str().unwrap().into()
        }
    };

    Block::new(&name).and_then(|ref block| block.size())
}

#[cfg(test)]
mod tests {
    use super::*;
    use operations::*;
    use partition_identity::PartitionIdentifiers;
    use std::collections::HashMap;

    fn get_default() -> Disks {
        Disks {
            physical: vec![Disk {
                mklabel:     false,
                model_name:  "Test Disk".into(),
                serial:      "Test Disk 123".into(),
                device_path: "/dev/sdz".into(),
                file_system: None,
                mount_point: vec![],
                size:        1953525168,
                device_type: "TEST".into(),
                table_type:  Some(PartitionTable::Gpt),
                read_only:   false,
                partitions:  vec![
                    PartitionInfo {
                        bitflags:     ACTIVE | BUSY | SOURCE,
                        device_path:  Path::new("/dev/sdz1").to_path_buf(),
                        flags:        vec![],
                        mount_point:  vec!(Path::new("/boot/efi").to_path_buf()),
                        target:       Some(Path::new("/boot/efi").to_path_buf()),
                        start_sector: 2048,
                        end_sector:   1026047,
                        filesystem:   Some(FileSystem::Fat16),
                        name:         None,
                        number:       1,
                        ordering:     1,
                        part_type:    PartitionType::Primary,
                        key_id:       None,
                        original_vg:  None,
                        lvm_vg:       None,
                        encryption:   None,
                        identifiers:  PartitionIdentifiers::default(),
                        subvolumes:   HashMap::new(),
                    },
                    PartitionInfo {
                        bitflags:     ACTIVE | BUSY | SOURCE,
                        device_path:  Path::new("/dev/sdz2").to_path_buf(),
                        flags:        vec![],
                        mount_point:  vec!(Path::new("/").to_path_buf()),
                        target:       Some(Path::new("/").to_path_buf()),
                        start_sector: 1026048,
                        end_sector:   420456447,
                        filesystem:   Some(FileSystem::Btrfs),
                        name:         Some("Pop!_OS".into()),
                        number:       2,
                        ordering:     2,
                        part_type:    PartitionType::Primary,
                        key_id:       None,
                        original_vg:  None,
                        lvm_vg: None,
                        encryption: None,
                        identifiers:  PartitionIdentifiers::default(),
                        subvolumes:   HashMap::new(),
                    },
                    PartitionInfo {
                        bitflags:     SOURCE,
                        device_path:  Path::new("/dev/sdz3").to_path_buf(),
                        flags:        vec![],
                        mount_point:  vec![],
                        target:       None,
                        start_sector: 420456448,
                        end_sector:   1936738303,
                        filesystem:   Some(FileSystem::Ext4),
                        name:         Some("Solus OS".into()),
                        number:       3,
                        ordering:     3,
                        part_type:    PartitionType::Primary,
                        key_id:       None,
                        original_vg:  None,
                        lvm_vg: None,
                        encryption: None,
                        identifiers:  PartitionIdentifiers::default(),
                        subvolumes:   HashMap::new(),
                    },
                    PartitionInfo {
                        bitflags:     ACTIVE | SOURCE,
                        device_path:  Path::new("/dev/sdz4").to_path_buf(),
                        flags:        vec![],
                        mount_point:  Vec::new(),
                        target:       None,
                        start_sector: 1936738304,
                        end_sector:   1953523711,
                        filesystem:   Some(FileSystem::Swap),
                        name:         None,
                        number:       4,
                        ordering:     4,
                        part_type:    PartitionType::Primary,
                        key_id:       None,
                        original_vg:  None,
                        lvm_vg: None,
                        encryption: None,
                        identifiers:  PartitionIdentifiers::default(),
                        subvolumes:   HashMap::new(),
                    },
                ],
            }],
            logical:  Vec::new(),
        }
    }

    fn get_empty() -> Disks {
        Disks {
            physical: vec![Disk {
                mklabel:     false,
                file_system: None,
                model_name:  "Test Disk".into(),
                serial:      "Test Disk 123".into(),
                device_path: "/dev/sdz".into(),
                mount_point: Vec::new(),
                size:        1953525168,
                device_type: "TEST".into(),
                table_type:  Some(PartitionTable::Gpt),
                read_only:   false,
                partitions:  Vec::new(),
            }],
            logical:  Vec::new(),
        }
    }

    const GIB20: u64 = 41943040;

    // 500 MiB Fat16 partition.
    fn boot_part(start: u64) -> PartitionBuilder {
        PartitionBuilder::new(start, 1_024_000 + start, FileSystem::Fat16)
    }

    // 20 GiB Ext4 partition.
    fn root_part(start: u64) -> PartitionBuilder {
        PartitionBuilder::new(start, GIB20 + start, FileSystem::Ext4)
    }

    #[test]
    fn layout_diff() {
        let source = get_default().physical.into_iter().next().unwrap();
        let mut new = source.clone();
        new.remove_partition(1).unwrap();
        new.remove_partition(2).unwrap();
        new.format_partition(3, FileSystem::Xfs).unwrap();
        let start = new.get_partition(3).unwrap().start_sector;
        new.resize_partition(3, start + GIB20).unwrap();
        new.remove_partition(4).unwrap();
        new.add_partition(boot_part(2048)).unwrap();
        new.add_partition(root_part(1_026_048)).unwrap();
        assert_eq!(
            source.diff(&new).unwrap(),
            DiskOps {
                mklabel:           None,
                device_path:       Path::new("/dev/sdz"),
                remove_partitions: vec![2048, 1026048, 420456448, 1936738304],
                change_partitions: vec![],
                create_partitions: vec![
                    PartitionCreate {
                        start_sector: 420456448,
                        end_sector:   420456447 + GIB20,
                        file_system:  Some(FileSystem::Xfs),
                        kind:         PartitionType::Primary,
                        flags:        vec![],
                        format:       true,
                        label:        None,
                        path:         PathBuf::from("/dev/sdz"),
                    },
                    PartitionCreate {
                        start_sector: 2048,
                        end_sector:   1_024_000 + 2047,
                        file_system:  Some(FileSystem::Fat16),
                        kind:         PartitionType::Primary,
                        flags:        vec![],
                        format:       true,
                        label:        None,
                        path:         PathBuf::from("/dev/sdz"),
                    },
                    PartitionCreate {
                        start_sector: 1_026_048,
                        end_sector:   GIB20 + 1_026_047,
                        file_system:  Some(FileSystem::Ext4),
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
        assert!(source
            .add_partition(PartitionBuilder::new(2048, 2_000_000, FileSystem::Ext4))
            .is_err());

        // Failures should also occur if the end sector exceeds the size of
        assert!(source
            .add_partition(PartitionBuilder::new(2048, 1953525169, FileSystem::Ext4))
            .is_err());

        // An empty disk should succeed, on the other hand.
        let mut source = get_empty().physical.into_iter().next().unwrap();

        // Create 500MiB Fat16 partition w/ 512 byte sectors.
        source.add_partition(boot_part(2048)).unwrap();

        // This should fail with an off by one error, due to the start
        // sector being located within the previous partition.
        assert!(source.add_partition(root_part(1_026_047)).is_err());

        // Create 20GiB Ext4 partition after that.
        source.add_partition(root_part(1_026_048)).unwrap();
    }

    #[test]
    fn layout_validity() {
        // This test ensures that invalid layouts will raise a flag. An invalid layout
        // is a layout which is missing some of the original source partitions.
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
            .add_partition(PartitionBuilder::new(2048, 1_024_000 + 2048, FileSystem::Fat16))
            .unwrap();
        assert!(source.validate_layout(&duplicate).is_ok());
    }
}
