bitflags! {
    pub struct FileSystemSupport: u8 {
        const LVM = 1;
        const LUKS = 2;
        const FAT = 4;
        const XFS = 8;
        const EXT4 = 16;
        const BTRFS = 32;
        const NTFS = 64;
        const F2FS = 128;
    }
}
