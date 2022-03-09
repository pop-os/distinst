use super::{
    FileSystem, PartitionFlag, PartitionIdentifiers, PartitionInfo, PartitionType,
    FORMAT,
};
use crate::LuksEncryption;
use std::collections::HashMap;
use std::path::PathBuf;

/// Partition builders are supplied as inputs to `Disk::add_partition`.
pub struct PartitionBuilder {
    pub start_sector: u64,
    pub end_sector:   u64,
    pub filesystem:   Option<FileSystem>,
    pub part_type:    PartitionType,
    pub name:         Option<String>,
    pub flags:        Vec<PartitionFlag>,
    pub mount:        Option<PathBuf>,
    pub lvm_vg:       Option<String>,
    pub encryption:   Option<LuksEncryption>,
    pub key_id:       Option<String>,
    pub subvolumes:   HashMap<PathBuf, String>
}

impl PartitionBuilder {
    /// Creates a new partition builder.
    pub fn new<O: Into<Option<FileSystem>>>(start: u64, end: u64, fs: O) -> PartitionBuilder {
        PartitionBuilder {
            start_sector: start,
            end_sector:   end - 1,
            filesystem:   fs.into(),
            part_type:    PartitionType::Primary,
            name:         None,
            flags:        Vec::new(),
            mount:        None,
            lvm_vg:       None,
            encryption:   None,
            key_id:       None,
            subvolumes:   HashMap::new()
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
    ) -> PartitionBuilder {
        self.lvm_vg = Some(group);
        self
    }

    pub fn encryption(
        mut self,
        encryption: LuksEncryption,
    ) -> PartitionBuilder {
        self.encryption = Some(encryption);
        self
    }

    /// Defines that this partition will store the keyfile of the given ID(s),
    /// at the target mount point.
    pub fn associate_keyfile(mut self, id: String) -> PartitionBuilder {
        self.key_id = Some(id);
        self
    }

    pub fn subvolume(mut self, target: impl Into<String>, volume: impl Into<String>) -> Self {
        self.subvolumes.insert(PathBuf::from(target.into()), volume.into());
        self
    }

    /// Builds a brand new Partition from the current state of the builder.
    pub fn build(self) -> PartitionInfo {
        PartitionInfo {
            bitflags:     FORMAT,
            number:       -1,
            start_sector: self.start_sector,
            end_sector:   self.end_sector,
            part_type:    self.part_type,
            filesystem: if self.encryption.is_some() {
                Some(FileSystem::Luks)
            } else if self.lvm_vg.is_some() {
                Some(FileSystem::Lvm)
            } else {
                self.filesystem
            },
            flags:        self.flags,
            name:         self.name,
            device_path:  PathBuf::new(),
            mount_point:  Vec::new(),
            ordering:     -1,
            target:       self.mount,
            original_vg:  None,
            lvm_vg:       self.lvm_vg,
            encryption:   self.encryption,
            key_id:       self.key_id,
            identifiers:  PartitionIdentifiers::default(),
            subvolumes:   self.subvolumes
        }
    }
}
