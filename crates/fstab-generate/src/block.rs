
use partition_identity::{PartitionID, PartitionSource};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use disk_types::FileSystem;

/// Information that will be used to generate a fstab entry for the given
/// partition.
#[derive(Debug, PartialEq)]
pub struct BlockInfo<'a> {
    pub uid:     PartitionID,
    mount:       Option<PathBuf>,
    pub fs:      &'static str,
    pub options: &'a str,
    pub dump:    bool,
    pub pass:    bool,
}

impl<'a> BlockInfo<'a> {
    pub fn new(
        uid: PartitionID,
        fs: FileSystem,
        target: Option<&Path>,
        options: &'a str,
    ) -> Self {
        BlockInfo {
            uid,
            mount: if fs == FileSystem::Swap {
                None
            } else {
                Some(target.expect("unable to get block info due to lack of target").to_path_buf())
            },
            fs: match fs {
                FileSystem::Fat16 | FileSystem::Fat32 => "vfat",
                FileSystem::Swap => "swap",
                _ => fs.into(),
            },
            options,
            dump: false,
            pass: false,
        }
    }

    /// Writes a single line to the fstab buffer for this file system.
    pub fn write_entry(&self, fstab: &mut OsString) {
        let mount_variant = match self.uid.variant {
            PartitionSource::ID => "ID=",
            PartitionSource::Label => "LABEL=",
            PartitionSource::PartLabel => "PARTLABEL=",
            PartitionSource::PartUUID => "PARTUUID=",
            PartitionSource::Path => "",
            PartitionSource::UUID => "UUID=",
        };

        fstab.push(mount_variant);
        fstab.push(&self.uid.id);
        fstab.push("  ");
        fstab.push(self.mount());
        fstab.push("  ");
        fstab.push(&self.fs);
        fstab.push("  ");
        fstab.push(&self.options);
        fstab.push("  ");
        fstab.push(if self.dump { "1" } else { "0" });
        fstab.push("  ");
        fstab.push(if self.pass { "1" } else { "0" });
        fstab.push("\n");
    }

    /// Retrieve the mount point, which is `none` if non-existent.
    pub fn mount(&self) -> &OsStr {
        self.mount
            .as_ref()
            .map_or(OsStr::new("none"), |path| path.as_os_str())
    }

    /// Helper for fetching the Partition ID of a partition.
    ///
    /// # Notes
    /// FAT partitions are prone to UUID collisions, so PartUUID will be used instead.
    pub fn get_partition_id(path: &Path, fs: FileSystem) -> Option<PartitionID> {
        if fs == FileSystem::Fat16 || fs == FileSystem::Fat32 {
            PartitionID::get_partuuid(path)
        } else {
            PartitionID::get_uuid(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn fstab_entries() {
        let swap_id = PartitionID { id: "SWAP".into(), variant: PartitionSource::UUID };
        let swap = BlockInfo::new(swap_id, FileSystem::Swap, None, "sw");
        let efi_id = PartitionID { id: "EFI".into(), variant: PartitionSource::PartUUID };
        let efi = BlockInfo::new(efi_id, FileSystem::Fat32, Some(Path::new("/boot/efi")), "defaults");
        let root_id = PartitionID { id: "ROOT".into(), variant: PartitionSource::UUID };
        let root = BlockInfo::new(root_id, FileSystem::Ext4, Some(Path::new("/")), "defaults");

        let fstab = &mut OsString::new();
        swap.write_entry(fstab);
        efi.write_entry(fstab);
        root.write_entry(fstab);

        assert_eq!(
            *fstab,
            OsString::from(r#"UUID=SWAP  none  swap  sw  0  0
PARTUUID=EFI  /boot/efi  vfat  defaults  0  0
UUID=ROOT  /  ext4  defaults  0  0
"#)
        );
    }

    #[test]
    fn block_info_swap() {
        let id = PartitionID {
            variant: PartitionSource::UUID,
            id: "TEST".to_owned()
        };
        let swap = BlockInfo::new(id, FileSystem::Swap, None, "sw");
        assert_eq!(
            swap,
            BlockInfo {
                uid: PartitionID {
                    variant: PartitionSource::UUID,
                    id: "TEST".to_owned()
                },
                mount: None,
                fs: "swap",
                options: "sw",
                dump: false,
                pass: false,
            }
        );
        assert_eq!(swap.mount(), OsStr::new("none"));
    }

    #[test]
    fn block_info_efi() {
        let id = PartitionID {
            variant: PartitionSource::PartUUID,
            id: "TEST".to_owned()
        };
        let efi = BlockInfo::new(id, FileSystem::Fat32, Some(Path::new("/boot/efi")), "defaults");
        assert_eq!(
            efi,
            BlockInfo {
                uid: PartitionID {
                    variant: PartitionSource::PartUUID,
                    id: "TEST".to_owned()
                },
                mount: Some(PathBuf::from("/boot/efi")),
                fs: "vfat",
                options: "defaults",
                dump: false,
                pass: false,
            }
        );
        assert_eq!(efi.mount(), OsStr::new("/boot/efi"));
    }

    #[test]
    fn block_info_root() {
        let id = PartitionID {
            variant: PartitionSource::UUID,
            id: "TEST".to_owned()
        };
        let root = BlockInfo::new(id, FileSystem::Ext4, Some(Path::new("/")), "defaults");
        assert_eq!(
            root,
            BlockInfo {
                uid: PartitionID {
                    variant: PartitionSource::UUID,
                    id: "TEST".to_owned()
                },
                mount: Some(PathBuf::from("/")),
                fs: FileSystem::Ext4.into(),
                options: "defaults",
                dump: false,
                pass: false,
            }
        );
        assert_eq!(root.mount(), OsStr::new("/"));
    }
}
