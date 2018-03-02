use super::{FileSystemType, LvmEncryption, PartitionFlag, PartitionInfo, PartitionType};
use std::path::PathBuf;

/// Partition builders are supplied as inputs to `Disk::add_partition`.
pub struct PartitionBuilder {
    pub(crate) start_sector: u64,
    pub(crate) end_sector:   u64,
    pub(crate) filesystem:   FileSystemType,
    pub(crate) part_type:    PartitionType,
    pub(crate) name:         Option<String>,
    pub(crate) flags:        Vec<PartitionFlag>,
    pub(crate) mount:        Option<PathBuf>,
    pub(crate) volume_group: Option<(String, Option<LvmEncryption>)>,
    pub(crate) key_id:       Option<(String, PathBuf)>,
}

impl PartitionBuilder {
    /// Creates a new partition builder.
    pub fn new(start: u64, end: u64, fs: FileSystemType) -> PartitionBuilder {
        PartitionBuilder {
            start_sector: start,
            end_sector:   end - 1,
            filesystem:   fs,
            part_type:    PartitionType::Primary,
            name:         None,
            flags:        Vec::new(),
            mount:        None,
            volume_group: None,
            key_id:       None,
        }
    }

    /// Defines a label for the new partition.
    pub fn name(mut self, name: String) -> PartitionBuilder {
        self.name = Some(name);
        self
    }

    /// Defines whether the partition shall be a logical or primary partition.
    pub fn partition_type(mut self, part_type: PartitionType) -> PartitionBuilder {
        self.part_type = part_type;
        self
    }

    /// Sets the input as the flags field for the new partition.
    pub fn flags(mut self, flags: Vec<PartitionFlag>) -> PartitionBuilder {
        self.flags = flags;
        self
    }

    /// Adds a partition flag for the new partition.
    pub fn flag(mut self, flag: PartitionFlag) -> PartitionBuilder {
        self.flags.push(flag);
        self
    }

    /// Specifies where the new partition should be mounted.
    pub fn mount(mut self, mount: PathBuf) -> PartitionBuilder {
        self.mount = Some(mount);
        self
    }

    /// Assigns the new partition to a LVM volume group, which may optionally
    /// be encrypted.
    pub fn logical_volume(
        mut self,
        group: String,
        encryption: Option<LvmEncryption>,
    ) -> PartitionBuilder {
        self.volume_group = Some((group, encryption));
        self
    }

    /// Defines that this partition will store the keyfile of the given ID(s),
    /// at the target mount point.
    pub fn keydata(mut self, id: String, target: PathBuf) -> PartitionBuilder {
        self.key_id = Some((id, target));
        self
    }

    /// Builds a brand new Partition from the current state of the builder.
    pub fn build(self) -> PartitionInfo {
        PartitionInfo {
            is_source:    false,
            remove:       false,
            format:       true,
            active:       false,
            busy:         false,
            number:       -1,
            start_sector: self.start_sector,
            end_sector:   self.end_sector,
            part_type:    self.part_type,
            filesystem:   if self.volume_group.is_some() {
                Some(FileSystemType::Lvm)
            } else {
                Some(self.filesystem)
            },
            flags:        self.flags,
            name:         self.name,
            device_path:  PathBuf::new(),
            mount_point:  None,
            swapped:      false,
            target:       if self.key_id.is_some() {
                self.key_id.as_ref().map(|&(_, ref path)| path.clone())
            } else {
                self.mount
            },
            volume_group: self.volume_group.clone(),
            key_id:       self.key_id,
        }
    }
}
