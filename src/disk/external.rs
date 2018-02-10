//! A collection of external commands used throughout the program.

use super::FileSystemType;
use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// Checks & corrects errors with partitions that have been moved / resized.
pub(crate) fn fsck<P: AsRef<Path>>(part: P, cmd: Option<(&str, &str)>) -> io::Result<()> {
    let part = part.as_ref();
    let status = Command::new(cmd.map_or("fsck", |(cmd, _)| cmd))
        .arg(cmd.map_or("-fy", |(_, args)| args))
        .arg(part)
        .stdout(Stdio::null())
        .status()?;
    if status.success() {
        info!("libdistinst: performed fsck on {}", part.display());
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("fsck on {} failed with status: {}", part.display(), status),
        ))
    }
}

/// Utilized for ensuring that block & partition information has synced with the OS.
pub fn blockdev<P: AsRef<Path>, S: AsRef<OsStr>, I: IntoIterator<Item = S>>(
    disk: P,
    args: I,
) -> io::Result<()> {
    let mut command = Command::new("blockdev");

    command.args(args);
    command.arg(disk.as_ref());

    debug!("{:?}", command);

    let status = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("blockdev failed with status: {}", status),
        ))
    }
}

/// Formats the supplied `part` device with the file system specified.
pub fn mkfs<P: AsRef<Path>>(part: P, kind: FileSystemType) -> io::Result<()> {
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
        FileSystemType::Xfs => ("mkfs.xfs", &["-f"]),
        FileSystemType::Lvm(..) => unimplemented!(),
    };

    let mut command = Command::new(cmd);
    command.arg(part);
    for arg in args {
        command.arg(arg);
    }

    debug!("{:?}", command);

    let status = command.stdout(Stdio::null()).status()?;

    if status.success() {
        info!("libdistinst: {} formatted with {:?}", part.display(), kind);
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("mkfs for {:?} failed with status: {}", kind, status),
        ))
    }
}

pub fn vgcreate<I: Iterator<Item = S>, S: AsRef<OsStr>>(group: &str, devices: I) -> io::Result<()> {
    Ok(())
}

pub fn lvcreate(group: &str, name: &str, size: u64) -> io::Result<()> { Ok(()) }
