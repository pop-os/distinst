use libparted::Partition;
use super::Mounts;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum FileSystemType {
    Btrfs,
    Exfat,
    Ext2,
    Ext3,
    Ext4,
    Fat16,
    Fat32,
    Swap,
    Xfs,
}

#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionType {
    Primary,
    Logical,
}

pub struct PartitionBuilder {
    pub(crate) start_sector: i64,
    pub(crate) end_sector: i64,
    filesystem: FileSystemType,
    part_type: PartitionType,
    name: Option<String>,
}

impl PartitionBuilder {
    pub fn new(start: i64, end: i64, fs: FileSystemType) -> PartitionBuilder {
        PartitionBuilder {
            start_sector: start,
            end_sector: end - 1,
            filesystem: fs,
            part_type: PartitionType::Primary,
            name: None,
        }
    }

    pub fn name(self, name: String) -> PartitionBuilder {
        PartitionBuilder {
            start_sector: self.start_sector,
            end_sector: self.end_sector,
            filesystem: self.filesystem,
            part_type: self.part_type,
            name: Some(name),
        }
    }

    pub fn partition_type(self, part_type: PartitionType) -> PartitionBuilder {
        PartitionBuilder {
            start_sector: self.start_sector,
            end_sector: self.end_sector,
            filesystem: self.filesystem,
            part_type,
            name: self.name,
        }
    }

    pub fn build(self) -> PartitionInfo {
        PartitionInfo {
            is_source: false,
            remove: false,
            active: false,
            busy: false,
            number: -1,
            start_sector: self.start_sector,
            end_sector: self.end_sector,
            part_type: self.part_type,
            filesystem: Some(self.filesystem),
            name: self.name,
            device_path: PathBuf::new(),
            mount_point: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartitionInfo {
    pub(crate) remove: bool,
    pub(crate) is_source: bool,
    pub active: bool,
    pub busy: bool,
    pub number: i32,
    pub start_sector: i64,
    pub end_sector: i64,
    pub part_type: PartitionType,
    pub filesystem: Option<FileSystemType>,
    pub name: Option<String>,
    pub device_path: PathBuf,
    pub mount_point: Option<PathBuf>,
}

impl PartitionInfo {
    pub fn new_from_ped(
        partition: &Partition,
        is_msdos: bool,
    ) -> io::Result<Option<PartitionInfo>> {
        let device_path = partition.get_path().unwrap().to_path_buf();
        let mounts = Mounts::new()?;

        Ok(Some(PartitionInfo {
            is_source: true,
            remove: false,
            part_type: match partition.type_get_name() {
                "primary" => PartitionType::Primary,
                "logical" => PartitionType::Logical,
                _ => return Ok(None),
            },
            mount_point: mounts.get_mount_point(&device_path),
            filesystem: partition.fs_type_name().and_then(FileSystemType::from),
            number: partition.num(),
            name: if is_msdos {
                None
            } else {
                partition.name().map(String::from)
            },
            // Note that primary and logical partitions should always have a path.
            device_path,
            active: partition.is_active(),
            busy: partition.is_busy(),
            start_sector: partition.geom_start(),
            end_sector: partition.geom_end(),
        }))
    }

    pub fn is_swap(&self) -> bool {
        self.filesystem
            .map_or(false, |fs| fs == FileSystemType::Swap)
    }

    pub fn path(&self) -> &Path {
        &self.device_path
    }

    pub(crate) fn requires_changes(&self, other: &PartitionInfo) -> bool {
        self.sectors_differ_from(other) || self.filesystem != other.filesystem
    }

    pub(crate) fn sectors_differ_from(&self, other: &PartitionInfo) -> bool {
        self.start_sector != other.start_sector || self.end_sector != other.end_sector
    }

    pub(crate) fn is_same_partition_as(&self, other: &PartitionInfo) -> bool {
        self.is_source && other.is_source && self.number == other.number
    }
}
