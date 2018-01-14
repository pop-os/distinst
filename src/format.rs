use std::io::{Error, ErrorKind, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use super::FileSystemType;

pub fn mkfs<P: AsRef<Path>>(part: P, kind: FileSystemType) -> Result<()> {
    let (command, args): (&str, Option<&[&str]>) = match kind {
        FileSystemType::Btrfs => ("mkfs.btrfs", Some(&["-f"])),
        FileSystemType::Exfat => ("mkfs.exfat", None),
        FileSystemType::Ext2 => ("mkfs.ext2", Some(&["-F", "-q"])),
        FileSystemType::Ext3 => ("mkfs.ext3", Some(&["-F", "-q"])),
        FileSystemType::Ext4 => ("mkfs.ext4", Some(&["-F", "-q"])),
        FileSystemType::F2fs => ("mkfs.f2fs", Some(&["-q"])),
        FileSystemType::Fat16 => ("mkfs.fat", Some(&["-F", "16"])),
        FileSystemType::Fat32 => ("mkfs.fat", Some(&["-F", "32"])),
        FileSystemType::Ntfs => ("mkfs.ntfs", Some(&["-F", "-q"])),
        FileSystemType::Swap => ("mkswap", Some(&["-f"])),
        FileSystemType::Xfs => ("mkfs.xfs", Some(&["-f"]))
    };
    
    let mut command = Command::new(command);
    args.map(|args| command.args(args));

    debug!("{:?}", command);

    let status = command.stdout(Stdio::null()).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("mkfs for {:?} failed with status: {}", kind, status)
        ))
    }
}
