use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use super::FileSystemType;

/// Information that will be used to generate a fstab entry for the given
/// partition.
#[derive(Debug, PartialEq)]
pub(crate) struct BlockInfo {
    pub uuid:    String,
    mount:       Option<PathBuf>,
    pub fs:      &'static str,
    pub options: String,
    pub dump:    bool,
    pub pass:    bool,
}

impl BlockInfo {
    pub fn new(
        uuid: String,
        fs: FileSystemType,
        target: Option<&Path>
    ) -> BlockInfo {
        BlockInfo {
            uuid,
            mount: if fs == FileSystemType::Swap {
                None
            } else {
                Some(target.expect("unable to get block info due to lack of target").to_path_buf())
            },
            fs: match fs {
                FileSystemType::Fat16 | FileSystemType::Fat32 => "vfat",
                FileSystemType::Swap => "swap",
                _ => fs.into(),
            },
            options: fs.get_preferred_options().into(),
            dump: false,
            pass: false,
        }
    }

    pub fn mount(&self) -> &OsStr {
        self.mount
            .as_ref()
            .map_or(OsStr::new("none"), |path| path.as_os_str())
    }

    /// The size of the data contained within.
    pub fn len(&self) -> usize {
        self.uuid.len() + self.mount().len() + self.fs.len() + self.options.len() + 2
    }

    pub fn write_fstab(&self, fstab: &mut OsString) {
        fstab.reserve_exact(self.len() + 16);
        fstab.push("UUID=");
        fstab.push(&self.uuid);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn fstab_entries() {
        let swap = BlockInfo::new("SWAP".into(), FileSystemType::Swap, None);
        let efi = BlockInfo::new("EFI".into(), FileSystemType::Fat32, Some(Path::new("/boot/efi")));
        let root = BlockInfo::new("ROOT".into(), FileSystemType::Ext4, Some(Path::new("/")));

        let fstab = &mut OsString::new();
        swap.write_fstab(fstab);
        efi.write_fstab(fstab);
        root.write_fstab(fstab);

        assert_eq!(
            *fstab,
            OsString::from(r#"UUID=SWAP  none  swap  sw  0  0
UUID=EFI  /boot/efi  vfat  umask=0077  0  0
UUID=ROOT  /  ext4  noatime,errors=remount-ro  0  0
"#)
        );
    }

    #[test]
    fn block_info_swap() {
        let swap = BlockInfo::new("TEST".into(), FileSystemType::Swap, None);
        assert_eq!(
            swap,
            BlockInfo {
                uuid: "TEST".into(),
                mount: None,
                fs: "swap",
                options: FileSystemType::Swap.get_preferred_options().into(),
                dump: false,
                pass: false,
            }
        );
        assert_eq!(swap.mount(), OsStr::new("none"));
        assert_eq!(swap.len(), 16);
    }

    #[test]
    fn block_info_efi() {
        let efi = BlockInfo::new("TEST".into(), FileSystemType::Fat32, Some(Path::new("/boot/efi")));
        assert_eq!(
            efi,
            BlockInfo {
                uuid: "TEST".into(),
                mount: Some(PathBuf::from("/boot/efi")),
                fs: "vfat",
                options: FileSystemType::Fat32.get_preferred_options().into(),
                dump: false,
                pass: false,
            }
        );
        assert_eq!(efi.mount(), OsStr::new("/boot/efi"));
        assert_eq!(efi.len(), 29);
    }

    #[test]
    fn block_info_root() {
        let root = BlockInfo::new("TEST".into(), FileSystemType::Ext4, Some(Path::new("/")));
        assert_eq!(
            root,
            BlockInfo {
                uuid: "TEST".into(),
                mount: Some(PathBuf::from("/")),
                fs: FileSystemType::Ext4.into(),
                options: FileSystemType::Ext4.get_preferred_options().into(),
                dump: false,
                pass: false,
            }
        );
        assert_eq!(root.mount(), OsStr::new("/"));
        assert_eq!(root.len(), 36);
    }
}
