use libparted::{Partition, PartitionFlag};
use super::Mounts;
use std::io;
use std::path::{Path, PathBuf};

/// Specifies which file system format to use.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum FileSystemType {
    Btrfs,
    Exfat,
    Ext2,
    Ext3,
    Ext4,
    F2fs,
    Fat16,
    Fat32,
    Ntfs,
    Swap,
    Xfs,
}

impl FileSystemType {
    fn from(string: &str) -> Option<FileSystemType> {
        let type_ = match string {
            "btrfs" => FileSystemType::Btrfs,
            "exfat" => FileSystemType::Exfat,
            "ext2" => FileSystemType::Ext2,
            "ext3" => FileSystemType::Ext3,
            "ext4" => FileSystemType::Ext4,
            "f2fs" => FileSystemType::F2fs,
            "fat16" => FileSystemType::Fat16,
            "fat32" => FileSystemType::Fat32,
            "linux-swap(v1)" => FileSystemType::Swap,
            "ntfs" => FileSystemType::Ntfs,
            "xfs" => FileSystemType::Xfs,
            _ => return None,
        };
        Some(type_)
    }
}

impl Into<&'static str> for FileSystemType {
    fn into(self) -> &'static str {
        match self {
            FileSystemType::Btrfs => "btrfs",
            FileSystemType::Exfat => "exfat",
            FileSystemType::Ext2 => "ext2",
            FileSystemType::Ext3 => "ext3",
            FileSystemType::Ext4 => "ext4",
            FileSystemType::F2fs => "f2fs",
            FileSystemType::Fat16 => "fat16",
            FileSystemType::Fat32 => "fat32",
            FileSystemType::Ntfs => "ntfs",
            FileSystemType::Swap => "linux-swap(v1)",
            FileSystemType::Xfs => "xfs",
        }
    }
}

/// Defines whether the partition is a primary or logical partition.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionType {
    Primary,
    Logical,
}

/// Partition builders are supplied as inputs to `Disk::add_partition`.
pub struct PartitionBuilder {
    pub(crate) start_sector: u64,
    pub(crate) end_sector: u64,
    pub(crate) filesystem: FileSystemType,
    pub(crate) part_type: PartitionType,
    pub(crate) name: Option<String>,
    pub(crate) flags: Vec<PartitionFlag>,
    pub(crate) mount: Option<PathBuf>,
}

impl PartitionBuilder {
    /// Creates a new partition builder.
    pub fn new(start: u64, end: u64, fs: FileSystemType) -> PartitionBuilder {
        PartitionBuilder {
            start_sector: start,
            end_sector: end - 1,
            filesystem: fs,
            part_type: PartitionType::Primary,
            name: None,
            flags: Vec::new(),
            mount: None,
        }
    }

    pub fn name(mut self, name: String) -> PartitionBuilder {
        self.name = Some(name);
        self
    }

    pub fn partition_type(mut self, part_type: PartitionType) -> PartitionBuilder {
        self.part_type = part_type;
        self
    }

    pub fn flag(mut self, flag: PartitionFlag) -> PartitionBuilder {
        self.flags.push(flag);
        self
    }

    pub fn mount(mut self, mount: PathBuf) -> PartitionBuilder {
        self.mount = Some(mount);
        self
    }

    pub fn build(self) -> PartitionInfo {
        PartitionInfo {
            is_source: false,
            remove: false,
            format: true,
            active: false,
            busy: false,
            number: -1,
            start_sector: self.start_sector,
            end_sector: self.end_sector,
            part_type: self.part_type,
            filesystem: Some(self.filesystem),
            flags: self.flags,
            name: self.name,
            device_path: PathBuf::new(),
            mount_point: None,
            target: self.mount,
        }
    }
}

// TODO: Compress boolean fields into a single byte.

/// Contains relevant information about a certain partition.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionInfo {
    /// If set to true, this is a source partition, which means it currently exists on the disk.
    pub(crate) is_source: bool,
    /// Source partitions will set this field. If set, this partition will be removed.
    pub(crate) remove: bool,
    /// Whether the filesystem should be formatted or not.
    pub format: bool,
    /// If the partition is currently active, this will be true.
    pub active: bool,
    /// If the partition is currently busy, this will be true.
    pub busy: bool,
    /// The partition number is the numeric value that follows the disk's device path.
    /// IE: _/dev/sda1_
    pub number: i32,
    /// The initial sector where the partition currently, or will, reside.
    pub start_sector: u64,
    /// The final sector where the partition currently, or will, reside.
    /// # Note
    /// The length of the partion can be calculated by substracting the `end_sector`
    /// from the `start_sector`, and multiplying that by the value of the disk's
    /// sector size.
    pub end_sector: u64,
    /// Whether this partition is a primary or logical partition.
    pub part_type: PartitionType,
    /// Whether there is a file system currently, or will be, on this partition.
    pub filesystem: Option<FileSystemType>,
    /// Specifies optional flags that should be applied to the partition, if not already set.
    pub flags: Vec<PartitionFlag>,
    /// Specifies the name of the partition.
    pub name: Option<String>,
    /// Contains the device path of the partition, which is the disk's device path plus
    /// the partition number.
    pub device_path: PathBuf,
    /// Where this partition is mounted in the file system, if at all.
    pub mount_point: Option<PathBuf>,
    /// Where this partition will be mounted in the future
    pub target: Option<PathBuf>,
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
            format: false,
            part_type: match partition.type_get_name() {
                "primary" => PartitionType::Primary,
                "logical" => PartitionType::Logical,
                _ => return Ok(None),
            },
            mount_point: mounts.get_mount_point(&device_path),
            target: None,
            filesystem: partition.fs_type_name().and_then(FileSystemType::from),
            flags: get_flags(&partition),
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
            start_sector: partition.geom_start() as u64,
            end_sector: partition.geom_end() as u64,
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

    pub fn set_mount(&mut self, target: PathBuf) {
        self.target = Some(target);
    }
}

const FLAGS: &[PartitionFlag] = &[
    PartitionFlag::PED_PARTITION_BOOT,
    PartitionFlag::PED_PARTITION_ROOT,
    PartitionFlag::PED_PARTITION_SWAP,
    PartitionFlag::PED_PARTITION_HIDDEN,
    PartitionFlag::PED_PARTITION_RAID,
    PartitionFlag::PED_PARTITION_LVM,
    PartitionFlag::PED_PARTITION_LBA,
    PartitionFlag::PED_PARTITION_HPSERVICE,
    PartitionFlag::PED_PARTITION_PALO,
    PartitionFlag::PED_PARTITION_PREP,
    PartitionFlag::PED_PARTITION_MSFT_RESERVED,
    PartitionFlag::PED_PARTITION_BIOS_GRUB,
    PartitionFlag::PED_PARTITION_APPLE_TV_RECOVERY,
    PartitionFlag::PED_PARTITION_DIAG,
    PartitionFlag::PED_PARTITION_LEGACY_BOOT,
    PartitionFlag::PED_PARTITION_MSFT_DATA,
    PartitionFlag::PED_PARTITION_IRST,
    PartitionFlag::PED_PARTITION_ESP,
];

fn get_flags(partition: &Partition) -> Vec<PartitionFlag> {
    FLAGS
        .into_iter()
        .filter(|&&f| partition.is_flag_available(f) && partition.get_flag(f))
        .cloned()
        .collect::<Vec<PartitionFlag>>()
}
