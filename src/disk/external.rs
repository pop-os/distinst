//! A collection of external commands used throughout the program.

use super::config::lvm::deactivate_devices;
use super::{FileSystemType, LvmEncryption};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::{File, Permissions};
use std::io::Read;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempdir::TempDir;

/// A generic function for executing a variety external commands.
fn exec(
    cmd: &str,
    stdin: Option<&[u8]>,
    valid_codes: Option<&'static [i32]>,
    args: &[OsString],
) -> io::Result<()> {
    info!("libdistinst: executing {} with {:?}", cmd, args);

    let mut child = Command::new(cmd)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .spawn()?;

    if let Some(stdin) = stdin {
        child
            .stdin
            .as_mut()
            .expect("stdin not obtained")
            .write_all(stdin)?;
    }

    let status = child.wait()?;
    let success = status.success() || valid_codes.map_or(false, |codes| {
        status.code().map_or(false, |code| codes.contains(&code))
    });

    if success {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "{} failed with status: {}",
                cmd,
                match status.code() {
                    Some(code) => format!("{} ({})", code, io::Error::from_raw_os_error(code)),
                    None => "unknown".into(),
                }
            ),
        ))
    }
}

/// Erase all signatures on a disk
pub(crate) fn wipefs<P: AsRef<Path>>(device: P) -> io::Result<()> {
    info!("libdistinst: using wipefs to wipe signatures from {:?}", device.as_ref());
    exec("wipefs", None, None, &["-a".into(), device.as_ref().into()])
}

/// Checks & corrects errors with partitions that have been moved / resized.
pub(crate) fn fsck<P: AsRef<Path>>(part: P, cmd: Option<(&str, &str)>) -> io::Result<()> {
    let (cmd, arg) = cmd.unwrap_or(("fsck", "-fy"));
    exec(cmd, None, None, &[arg.into(), part.as_ref().into()])
}

/// Utilized for ensuring that block & partition information has synced with
/// the OS.
pub(crate) fn blockdev<P: AsRef<Path>, S: AsRef<OsStr>, I: IntoIterator<Item = S>>(
    disk: P,
    args: I,
) -> io::Result<()> {
    exec("blockdev", None, None, &{
        let mut args = args.into_iter()
            .map(|x| x.as_ref().into())
            .collect::<Vec<OsString>>();
        args.push(disk.as_ref().into());
        args
    })
}

pub(crate) fn dmlist() -> io::Result<Vec<String>> {
    let mut current_line = String::with_capacity(64);
    let mut output = Vec::new();

    let mut reader = BufReader::new(
        Command::new("dmsetup")
            .arg("ls")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .expect("failed to execute dmsetup command"),
    );

    // Skip the first line of output
    let _ = reader.read_line(&mut current_line);
    current_line.clear();

    while reader.read_line(&mut current_line)? != 0 {
        {
            let mut fields = current_line.split_whitespace();
            if let Some(dm) = fields.next() {
                output.push(dm.into());
            }
        }

        current_line.clear();
    }

    Ok(output)
}

fn swap_exists(path: &Path) -> bool {
    Command::new("swaplabel").arg(path).status().ok().map_or(false, |stat| stat.success())
}

/// Formats the supplied `part` device with the file system specified.
pub(crate) fn mkfs<P: AsRef<Path>>(part: P, kind: FileSystemType) -> io::Result<()> {
    use FileSystemType::*;
    let (cmd, args): (&'static str, &'static [&'static str]) = match kind {
        Btrfs => ("mkfs.btrfs", &["-f"]),
        Exfat => ("mkfs.exfat", &[]),
        Ext2 => ("mkfs.ext2", &["-F", "-q"]),
        Ext3 => ("mkfs.ext3", &["-F", "-q"]),
        Ext4 => ("mkfs.ext4", &["-F", "-q", "-E", "lazy_itable_init"]),
        F2fs => ("mkfs.f2fs", &["-q"]),
        Fat16 => ("mkfs.fat", &["-F", "16"]),
        Fat32 => ("mkfs.fat", &["-F", "32"]),
        Ntfs => ("mkfs.ntfs", &["-FQ", "-q"]),
        Swap => {
            if swap_exists(part.as_ref()) {
                return Ok(());
            }

            ("mkswap", &["-f"])
        },
        Xfs => ("mkfs.xfs", &["-f"]),
        Luks | Lvm => return Ok(()),
    };

    exec(cmd, None, None, &{
        let mut args = args.into_iter().map(Into::into).collect::<Vec<OsString>>();
        args.push(part.as_ref().into());
        args
    })
}

fn get_label_cmd(kind: FileSystemType) -> Option<(&'static str, &'static [&'static str])> {
    use FileSystemType::*;
    let cmd: (&'static str, &'static [&'static str]) = match kind {
        Btrfs => ("btrfs", &["filesystem", "label"]),
        Exfat => ("exfatlabel", &[]),
        Ext2 | Ext3 | Ext4 => ("e2label", &[]),
        F2fs => unimplemented!(),
        Fat16 | Fat32 => ("dosfslabel", &[]),
        Ntfs => ("ntfslabel", &[]),
        Xfs => ("xfs_admin", &["-l"]),
        Swap | Luks | Lvm => {
            return None;
        }
    };

    Some(cmd)
}

/// Formats the supplied `part` device with the file system specified.
pub(crate) fn get_label<P: AsRef<Path>>(part: P, kind: FileSystemType) -> Option<String> {
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

    let output: String = if kind == FileSystemType::Xfs {
        if output.len() > 10 {
            output[9..output.len() - 2].into()
        } else {
            return None;
        }
    } else {
        if !output.is_empty() {
            output.trim_right().into()
        } else {
            return None;
        }
    };

    Some(output)
}

/// Used to create a physical volume on a LUKS partition.
pub(crate) fn pvcreate<P: AsRef<Path>>(device: P) -> io::Result<()> {
    exec(
        "pvcreate",
        None,
        None,
        &["-ffy".into(), device.as_ref().into()],
    )
}

/// Used to create a volume group from one or more physical volumes.
pub(crate) fn vgcreate<I: Iterator<Item = S>, S: AsRef<OsStr>>(
    group: &str,
    devices: I,
) -> io::Result<()> {
    exec("vgcreate", None, None, &{
        let mut args = Vec::with_capacity(16);
        args.push("-ffy".into());
        args.push(group.into());
        args.extend(devices.map(|x| x.as_ref().into()));
        args
    })
}

/// Removes the given volume group from the system.
pub(crate) fn vgremove(group: &str) -> io::Result<()> {
    exec("vgremove", None, None, &["-ffy".into(), group.into()])
}

/// Used to create a logical volume on a volume group.
pub(crate) fn lvcreate(group: &str, name: &str, size: Option<u64>) -> io::Result<()> {
    exec(
        "lvcreate",
        None,
        None,
        &size.map_or(
            [
                "-y".into(),
                "-l".into(),
                "100%FREE".into(),
                group.into(),
                "-n".into(),
                name.into(),
            ],
            |size| {
                [
                    "-y".into(),
                    "-L".into(),
                    mebibytes(size).into(),
                    group.into(),
                    "-n".into(),
                    name.into(),
                ]
            },
        ),
    )
}

pub(crate) fn lvremove(group: &str, name: &str) -> io::Result<()> {
    exec(
        "lvremove",
        None,
        None,
        &[
            "-y".into(),
            ["/dev/mapper/", group, "-", name].concat().into(),
        ],
    )
}

/// Append a newline to the input (used for the password)
fn append_newline(input: &[u8]) -> Vec<u8> {
    let mut input = input.to_owned();
    input.reserve_exact(1);
    input.push(b'\n');
    input
}

// TODO: Possibly use the cryptsetup crate instead of the cryptsetup command?

/// Creates a LUKS partition from a physical partition. This could be either a LUKS on LVM
/// configuration, or a LVM on LUKS configurations.
pub(crate) fn cryptsetup_encrypt(device: &Path, enc: &LvmEncryption) -> io::Result<()> {
    info!(
        "libdistinst: cryptsetup is encrypting {} with {:?}",
        device.display(),
        enc
    );
    match (enc.password.as_ref(), enc.keydata.as_ref()) {
        (Some(_password), Some(_keydata)) => unimplemented!(),
        (Some(password), None) => exec(
            "cryptsetup",
            Some(&append_newline(password.as_bytes())),
            None,
            &[
                "-s".into(),
                "512".into(),
                "luksFormat".into(),
                device.into(),
            ],
        ),
        (None, Some(&(_, ref keydata))) => {
            let keydata = keydata.as_ref().expect("field should have been populated");
            let tmpfs = TempDir::new("distinst")?;
            let _mount = ExternalMount::new(&keydata.0, tmpfs.path(), LAZY)?;
            let keypath = tmpfs.path().join(&enc.physical_volume);

            generate_keyfile(&keypath)?;
            info!("libdistinst: keypath exists: {}", keypath.is_file());

            exec(
                "cryptsetup",
                None,
                None,
                &[
                    "-s".into(),
                    "512".into(),
                    "luksFormat".into(),
                    device.into(),
                    tmpfs.path().join(&enc.physical_volume).into(),
                ],
            )
        }
        (None, None) => unimplemented!(),
    }
}

/// Opens an encrypted partition and maps it to the pv name.
pub(crate) fn cryptsetup_open(device: &Path, enc: &LvmEncryption) -> io::Result<()> {
    deactivate_devices(&[device])?;
    let pv = &enc.physical_volume;
    info!(
        "libdistinst: cryptsetup is opening {} with pv {} and {:?}",
        device.display(),
        pv,
        enc
    );
    match (enc.password.as_ref(), enc.keydata.as_ref()) {
        (Some(_password), Some(_keydata)) => unimplemented!(),
        (Some(password), None) => exec(
            "cryptsetup",
            Some(&append_newline(password.as_bytes())),
            None,
            &["open".into(), device.into(), pv.into()],
        ),
        (None, Some(&(_, ref keydata))) => {
            let keydata = keydata.as_ref().expect("field should have been populated");
            let tmpfs = TempDir::new("distinst")?;
            let _mount = ExternalMount::new(&keydata.0, tmpfs.path(), LAZY)?;
            let keypath = tmpfs.path().join(&enc.physical_volume);
            info!("libdistinst: keypath exists: {}", keypath.is_file());

            exec(
                "cryptsetup",
                None,
                None,
                &[
                    "open".into(),
                    device.into(),
                    pv.into(),
                    "--key-file".into(),
                    keypath.into(),
                ],
            )
        }
        (None, None) => unimplemented!(),
    }
}

/// If `cryptsetup info DEV` has an exit status of 0, the partition is encrypted.
pub(crate) fn is_encrypted(device: &Path) -> bool {
    let mut attempts = 0;
    loop {
        let res = Command::new("cryptsetup")
            .stdout(Stdio::null())
            .arg("luksDump")
            .arg(device)
            .status()
            .ok();

        match res.and_then(|stat| stat.code()) {
            Some(0) => return true,
            // An exit status of 4 can happen if the partition is scanned too hastily.
            Some(4) => {
                thread::sleep(Duration::from_millis(100));
                if attempts == 3 {
                    return false;
                }
                attempts += 1;
                continue
            },
            _ => return false,
        }
    }
}

/// Closes an encrypted partition.
pub(crate) fn cryptsetup_close(device: &Path) -> io::Result<()> {
    let args = &["close".into(), device.into()];
    exec("cryptsetup", None, Some(&[4]), args)
}

/// Deactivates all logical volumes in the supplied volume group
pub(crate) fn vgactivate(volume_group: &str) -> io::Result<()> {
    info!("libdistinst: activating '{}'", volume_group);
    let args = &["-ffyay".into(), volume_group.into()];
    exec("vgchange", None, None, args)
}

/// Deactivates all logical volumes in the supplied volume group
pub(crate) fn vgdeactivate(volume_group: &str) -> io::Result<()> {
    info!("libdistinst: deactivating '{}'", volume_group);
    let args = &["-ffyan".into(), volume_group.into()];
    exec("vgchange", None, None, args)
}

/// Removes the physical volume from the system.
pub(crate) fn pvremove(physical_volume: &Path) -> io::Result<()> {
    let args = &["-ffy".into(), physical_volume.into()];
    exec("pvremove", None, None, args)
}

/// Obtains the file system on a partition via blkid
pub(crate) fn blkid_partition<P: AsRef<Path>>(part: P) -> Option<FileSystemType> {
    let output = Command::new("blkid")
        .arg(part.as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?
        .stdout;

    String::from_utf8_lossy(&output)
        .split_whitespace()
        .skip(2)
        .next()
        .and_then(|type_| {
            info!("libdistinst: blkid found '{}'", type_);
            let length = type_.len();
            if length > 7 {
                type_[6..length - 1].parse::<FileSystemType>().ok()
            } else {
                None
            }
        })
}

/// Obtains a list of logical volumes associated with the given volume group.
pub(crate) fn lvs(vg: &str) -> io::Result<Vec<PathBuf>> {
    info!("libdistinst: obtaining logical volumes on {}", vg);
    let mut current_line = String::with_capacity(128);
    let mut output = Vec::new();

    let mut reader = BufReader::new(
        Command::new("lvs")
            .arg(vg)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .expect("failed to execute lvs command"),
    );

    // Skip the first line of output
    let _ = reader.read_line(&mut current_line);
    current_line.clear();

    while reader.read_line(&mut current_line)? != 0 {
        {
            let line = &current_line[2..];
            match line.find(' ') {
                Some(pos) => output.push(PathBuf::from(
                    [
                        "/dev/mapper/",
                        &vg.replace("-", "--"),
                        "-",
                        &(&line[..pos].replace("-", "--"))
                    ].concat(),
                )),
                None => (),
            }
        }

        current_line.clear();
    }

    Ok(output)
}

/// Obtains a map of physical volume paths and their optionally-assigned volume
/// groups.
pub(crate) fn pvs() -> io::Result<BTreeMap<PathBuf, Option<String>>> {
    info!("libdistinst: obtaining list of physical volumes");
    let mut current_line = String::with_capacity(64);
    let mut output = BTreeMap::new();

    let mut reader = BufReader::new(
        Command::new("pvs")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .expect("failed to execute pvs command"),
    );

    // Skip the first line of output
    let _ = reader.read_line(&mut current_line);
    current_line.clear();

    while reader.read_line(&mut current_line)? != 0 {
        {
            let mut fields = current_line[2..].split_whitespace();
            fields.next().map(|pv| {
                fields.next().map(|vg| {
                    output.insert(
                        PathBuf::from(pv),
                        if vg.is_empty() || vg == "lvm2" {
                            None
                        } else {
                            Some(vg.into())
                        },
                    )
                })
            });
        }

        current_line.clear();
    }

    Ok(output)
}

fn mebibytes(bytes: u64) -> String { format!("{}", bytes / (1024 * 1024)) }

/// Generates a new keyfile by reading 512 bytes from "/dev/urandom".
fn generate_keyfile(path: &Path) -> io::Result<()> {
    info!("libdistinst: generating keyfile at {}", path.display());
    // Generate the key in memory from /dev/urandom.
    let mut key = [0u8; 512];
    let mut urandom = File::open("/dev/urandom")?;
    urandom.read_exact(&mut key)?;

    // Open the keyfile and write the key, ensuring it is readable only to root.
    let mut keyfile = File::create(path)?;
    keyfile.set_permissions(Permissions::from_mode(0o0400))?;
    keyfile.write_all(&key)?;
    keyfile.sync_all()
}

pub(crate) const BIND: u8 = 0b01;
pub(crate) const LAZY: u8 = 0b10;

pub(crate) struct ExternalMount<'a> {
    dest:  &'a Path,
    flags: u8,
}

impl<'a> ExternalMount<'a> {
    pub(crate) fn new(src: &'a Path, dest: &'a Path, flags: u8) -> io::Result<ExternalMount<'a>> {
        let args = if flags & BIND != 0 {
            vec!["--bind".into(), src.into(), dest.into()]
        } else {
            vec![src.into(), dest.into()]
        };

        exec("mount", None, None, &args).map(|_| ExternalMount { dest, flags })
    }
}

impl<'a> Drop for ExternalMount<'a> {
    fn drop(&mut self) {
        let args = if self.flags & LAZY != 0 {
            vec!["-l".into(), self.dest.into()]
        } else {
            vec![self.dest.into()]
        };

        let _ = exec("umount", None, None, &args);
    }
}

pub(crate) fn remount_rw<P: AsRef<Path>>(path: P) -> io::Result<()> {
    exec("mount", None, None, &[path.as_ref().into(), "-o".into(), "remount,rw".into()])
}
