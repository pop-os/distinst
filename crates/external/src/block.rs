use self::FileSystem::*;
use super::exec;
use disk_types::FileSystem;
use crate::retry::Retry;
use std::{
    ffi::{OsStr, OsString},
    io,
    path::Path,
    process::{Command, Stdio},
};

/// Erase all signatures on a disk
pub fn wipefs<P: AsRef<Path>>(device: P) -> io::Result<()> {
    info!("using wipefs to wipe signatures from {:?}", device.as_ref());
    exec("wipefs", None, None, &["-a".into(), device.as_ref().into()])
}

/// Utilized for ensuring that block & partition information has synced with
/// the OS.
pub fn blockdev<P: AsRef<Path>, S: AsRef<OsStr>, I: IntoIterator<Item = S>>(
    disk: P,
    args: I,
) -> io::Result<()> {
    exec("blockdev", None, None, &{
        let mut args = args.into_iter().map(|x| x.as_ref().into()).collect::<Vec<OsString>>();
        args.push(disk.as_ref().into());
        args
    })
}

/// Obtains the file system on a partition via blkid
pub fn blkid_partition<P: AsRef<Path>>(part: P) -> Option<FileSystem> {
    let output = Command::new("blkid")
        .arg(part.as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?
        .stdout;

    for field in String::from_utf8_lossy(&output).split_whitespace() {
        if field.starts_with("TYPE=") {
            let length = field.len();
            return if length > 7 {
                field[6..length - 1].parse::<FileSystem>().ok()
            } else {
                None
            }
        }
    }

    return None
}

/// Checks & corrects errors with partitions that have been moved / resized.
pub fn fsck<P: AsRef<Path>>(part: P, cmd: Option<(&str, &str)>) -> io::Result<()> {
    let (cmd, arg) = cmd.unwrap_or(("fsck", "-fy"));

    Retry::default()
        .attempts(3)
        .interval(1000)
        .retry_until_ok(move || exec(cmd, None, None, &[arg.into(), part.as_ref().into()]))
}

/// Formats the supplied `part` device with the file system specified.
pub fn mkfs<P: AsRef<Path>>(part: P, kind: FileSystem) -> io::Result<()> {
    let (cmd, args): (&'static str, &'static [&'static str]) = match kind {
        Btrfs => ("mkfs.btrfs", &["-f"]),
        // Exfat => ("mkfs.exfat", &[]),
        Exfat => unimplemented!("exfat is not supported, yet"),
        Ext2 => ("mkfs.ext2", &["-F", "-q"]),
        Ext3 => ("mkfs.ext3", &["-F", "-q"]),
        Ext4 => ("mkfs.ext4", &["-F", "-q", "-E", "lazy_itable_init"]),
        F2fs => ("mkfs.f2fs", &["-f", "-q", "-O", "extra_attr,inode_checksum,sb_checksum,compression"]),
        Fat16 => ("mkfs.fat", &["-F", "16"]),
        Fat32 => ("mkfs.fat", &["-F", "32"]),
        Ntfs => ("mkfs.ntfs", &["-FQ", "-q"]),
        Swap => {
            if swap_exists(part.as_ref()) {
                return Ok(());
            }

            ("mkswap", &["-f"])
        }
        Xfs => ("mkfs.xfs", &["-f"]),
        Luks | Lvm => return Ok(()),
    };

    exec(cmd, None, None, &{
        let mut args = args.iter().map(Into::into).collect::<Vec<OsString>>();
        args.push(part.as_ref().into());
        args
    })
}

/// Get the label from the given partition, if it exists.
pub fn get_label<P: AsRef<Path>>(part: P, kind: FileSystem) -> Option<String> {
    let (cmd, args) = get_label_cmd(kind)?;

    let output = Command::new(cmd)
        .args(args)
        .arg(part.as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?
        .stdout;

    let output: String = String::from_utf8_lossy(&output).into();

    let output: String = if kind == FileSystem::Xfs {
        if output.len() > 10 {
            output[9..output.len() - 2].into()
        } else {
            return None;
        }
    } else if !output.is_empty() {
        output.trim_end().into()
    } else {
        return None;
    };

    Some(output)
}

fn get_label_cmd(kind: FileSystem) -> Option<(&'static str, &'static [&'static str])> {
    let cmd = match kind {
        Btrfs => ("btrfs", &["filesystem", "label"][..]),
        Ext2 | Ext3 | Ext4 => ("e2label", &[][..]),
        Fat16 | Fat32 => ("dosfslabel", &[][..]),
        Ntfs => ("ntfslabel", &[][..]),
        Xfs => ("xfs_admin", &["-l"][..]),
        Swap | Luks | Lvm => {
            return None;
        }
        _ => ("blkid", &["-s", "LABEL", "-o", "value"][..]),
    };

    Some(cmd)
}

pub fn remount_rw<P: AsRef<Path>>(path: P) -> io::Result<()> {
    exec("mount", None, None, &[path.as_ref().into(), "-o".into(), "remount,rw".into()])
}

fn swap_exists(path: &Path) -> bool {
    Command::new("swaplabel").arg(path).status().ok().map_or(false, |stat| stat.success())
}
