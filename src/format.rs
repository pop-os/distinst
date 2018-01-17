use std::io::{Error, ErrorKind, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use super::FileSystemType;

pub fn mkfs<P: AsRef<Path>>(part: P, kind: FileSystemType) -> Result<()> {
    let part = part.as_ref();

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
        FileSystemType::Xfs => ("mkfs.xfs", &["-f"])
    };

    let mut command = Command::new(cmd);
    eprintln!("Executing args: {:?}", args);
    command.arg(part);
    command.args(args);

    debug!("{} + {} + {:?} = {:?} ?", cmd, part.display(), args, command);

    let status = command.stdout(Stdio::null()).status()?;

    if status.success() {
        info!("{} formatted with {:?}", part.display(), kind);
        drop(command);
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("mkfs for {:?} failed with status: {}", kind, status)
        ))
    }
}
