use super::FileSystemType;
use std::io::{Error, ErrorKind, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn mkfs<P: AsRef<Path>>(part: P, kind: FileSystemType) -> Result<()> {
    let part = part.as_ref();
    let mut _volume_group = 0;

    let (cmd, args): (&'static str, &'static [&'static str]) = match kind {
        FileSystemType::Btrfs => ("mkfs.btrfs", &["-f"]),
        FileSystemType::Exfat => ("mkfs.exfat", &[]),
        FileSystemType::Ext2 => ("mkfs.ext2", &["-F", "-q"]),
        FileSystemType::Ext3 => ("mkfs.ext3", &["-F", "-q"]),
        FileSystemType::Ext4 => ("mkfs.ext4", &["-F", "-q"]),
        FileSystemType::F2fs => ("mkfs.f2fs", &["-q"]),
        FileSystemType::Fat16 => ("mkfs.fat", &["-F", "16"]),
        FileSystemType::Fat32 => ("mkfs.fat", &["-F", "32"]),
        FileSystemType::Ntfs => ("mkfs.ntfs", &["-F", "-q"]),
        FileSystemType::Swap => ("mkswap", &["-f"]),
        FileSystemType::Xfs => ("mkfs.xfs", &["-f"]),
        FileSystemType::Lvm(_gid) => unimplemented!()
    };

    let mut command = Command::new(cmd);
    command.arg(part);
    for arg in args {
        command.arg(arg);
    }

    debug!("{:?}", command);

    let status = command.stdout(Stdio::null()).status()?;

    if status.success() {
        info!("{} formatted with {:?}", part.display(), kind);
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("mkfs for {:?} failed with status: {}", kind, status),
        ))
    }
}
