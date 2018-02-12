//! A collection of external commands used throughout the program.

use super::FileSystemType;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

fn exec(cmd: &str, args: &[OsString]) -> io::Result<()> {
    info!("libdistinst: executing {} with {:?}", cmd, args);

    let status = Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("{} failed with status: {}", cmd, status),
        ))
    }
}

/// Checks & corrects errors with partitions that have been moved / resized.
pub(crate) fn fsck<P: AsRef<Path>>(part: P, cmd: Option<(&str, &str)>) -> io::Result<()> {
    let (cmd, arg) = cmd.unwrap_or(("fsck", "-fy"));
    exec(cmd, &vec![arg.into(), part.as_ref().into()])
}

/// Utilized for ensuring that block & partition information has synced with the OS.
pub fn blockdev<P: AsRef<Path>, S: AsRef<OsStr>, I: IntoIterator<Item = S>>(
    disk: P,
    args: I,
) -> io::Result<()> {
    exec("blockdev", &{
        let mut args = args.into_iter()
            .map(|x| x.as_ref().into())
            .collect::<Vec<OsString>>();
        args.push(disk.as_ref().into());
        args
    })
}

/// Formats the supplied `part` device with the file system specified.
pub fn mkfs<P: AsRef<Path>>(part: P, kind: FileSystemType) -> io::Result<()> {
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

    exec(cmd, &{
        let mut args = args.into_iter().map(Into::into).collect::<Vec<OsString>>();
        args.push(part.as_ref().into());
        args
    })
}

pub fn vgcreate<I: Iterator<Item = S>, S: AsRef<OsStr>>(group: &str, devices: I) -> io::Result<()> {
    Ok(())
}

pub fn lvcreate(group: &str, name: &str, size: u64) -> io::Result<()> { Ok(()) }
