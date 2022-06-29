use concat_in_place::strcat;
use partition_identity::{PartitionID, PartitionSource};
use std::path::{Path, PathBuf};
use disk_types::FileSystem;
use std::borrow::Cow;

/// Information that will be used to generate a fstab entry for the given
/// partition.
#[derive(Debug, PartialEq)]
pub struct BlockInfo<'a> {
    pub uid:     PartitionID,
    pub mount:   Option<PathBuf>,
    pub fs:      &'static str,
    pub options: Cow<'a, str>,
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
                target.map(Path::to_path_buf)
            },
            fs: match fs {
                FileSystem::Fat16 | FileSystem::Fat32 => "vfat",
                FileSystem::Swap => "swap",
                _ => fs.into(),
            },
            options: Cow::Borrowed(options),
            dump: false,
            pass: false,
        }
    }

    #[allow(unused_braces)]
    /// Writes a single line to the fstab buffer for this file system.
    pub fn write(&self, fstab: &mut String) {
        let mount_variant = match self.uid.variant {
            PartitionSource::ID => "ID=",
            PartitionSource::Label => "LABEL=",
            PartitionSource::PartLabel => "PARTLABEL=",
            PartitionSource::PartUUID => "PARTUUID=",
            PartitionSource::Path => "",
            PartitionSource::UUID => "UUID=",
        };

        let id = &self.uid.id;
        let mount = self.mount();

        strcat!(
            fstab,
            mount_variant id
            "  " mount
            "  " self.fs
            "  " {&self.options}
            "  " if self.dump { "1" } else { "0" }
            "  " if self.pass { "1" } else { "0" }
            "\n"
        );
    }

    /// Retrieve the mount point, which is `none` if non-existent.
    pub fn mount(&self) -> &str {
        self.mount
            .as_ref()
            .map_or("none", |path| path.as_os_str().to_str().unwrap())
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
        let root = BlockInfo::new(root_id, FileSystem::Btrfs, Some(Path::new("/")), "defaults");

        let fstab = &mut String::new();
        swap.write(fstab);
        efi.write(fstab);
        root.write(fstab);

        assert_eq!(
            *fstab,
            String::from(r#"UUID=SWAP  none  swap  sw  0  0
PARTUUID=EFI  /boot/efi  vfat  defaults  0  0
UUID=ROOT  /  btrfs  defaults  0  0
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
                options: Cow::Borrowed("sw"),
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
                options: Cow::Borrowed("defaults"),
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
                options: Cow::Borrowed("defaults"),
                dump: false,
                pass: false,
            }
        );
        assert_eq!(root.mount(), OsStr::new("/"));
    }
}
