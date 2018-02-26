//! A collection of external commands used throughout the program.

use super::{FileSystemType, LvmEncryption};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::{File, Permissions};
use std::io::{self, BufRead, BufReader, Write};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempdir::TempDir;

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

/// Checks & corrects errors with partitions that have been moved / resized.
pub(crate) fn fsck<P: AsRef<Path>>(part: P, cmd: Option<(&str, &str)>) -> io::Result<()> {
    let (cmd, arg) = cmd.unwrap_or(("fsck", "-fy"));
    exec(cmd, None, None, &[arg.into(), part.as_ref().into()])
}

/// Utilized for ensuring that block & partition information has synced with the OS.
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

/// Formats the supplied `part` device with the file system specified.
pub(crate) fn mkfs<P: AsRef<Path>>(part: P, kind: FileSystemType) -> io::Result<()> {
    let (cmd, args): (&'static str, &'static [&'static str]) = match kind {
        FileSystemType::Btrfs => ("mkfs.btrfs", &["-f"]),
        FileSystemType::Exfat => ("mkfs.exfat", &[]),
        FileSystemType::Ext2 => ("mkfs.ext2", &["-F", "-q"]),
        FileSystemType::Ext3 => ("mkfs.ext3", &["-F", "-q"]),
        FileSystemType::Ext4 => ("mkfs.ext4", &["-F", "-q"]),
        FileSystemType::F2fs => ("mkfs.f2fs", &["-q"]),
        FileSystemType::Fat16 => ("mkfs.fat", &["-F", "16"]),
        FileSystemType::Fat32 => ("mkfs.fat", &["-F", "32"]),
        FileSystemType::Ntfs => ("mkfs.ntfs", &["-FQ", "-q"]),
        FileSystemType::Swap => ("mkswap", &["-f"]),
        FileSystemType::Xfs => ("mkfs.xfs", &["-f"]),
        FileSystemType::Lvm => return Ok(()),
    };

    exec(cmd, None, None, &{
        let mut args = args.into_iter().map(Into::into).collect::<Vec<OsString>>();
        args.push(part.as_ref().into());
        args
    })
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
        (Some(password), Some(keydata)) => unimplemented!(),
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
            let _mount = ExternalMount::new(&keydata.0, tmpfs.path())?;
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

/// Opens an encrypted partition and maps it to the group name.
pub(crate) fn cryptsetup_open(device: &Path, group: &str, enc: &LvmEncryption) -> io::Result<()> {
    info!(
        "libdistinst: cryptsetup is opening {} with group {} and {:?}",
        device.display(),
        group,
        enc
    );
    match (enc.password.as_ref(), enc.keydata.as_ref()) {
        (Some(password), Some(keydata)) => unimplemented!(),
        (Some(password), None) => exec(
            "cryptsetup",
            Some(&append_newline(password.as_bytes())),
            None,
            &["open".into(), device.into(), group.into()],
        ),
        (None, Some(&(_, ref keydata))) => {
            let keydata = keydata.as_ref().expect("field should have been populated");
            let tmpfs = TempDir::new("distinst")?;
            let _mount = ExternalMount::new(&keydata.0, tmpfs.path())?;
            let keypath = tmpfs.path().join(&enc.physical_volume);
            info!("libdistinst: keypath exists: {}", keypath.is_file());

            exec(
                "cryptsetup",
                None,
                None,
                &[
                    "open".into(),
                    device.into(),
                    group.into(),
                    "--key-file".into(),
                    keypath.into(),
                ],
            )
        }
        (None, None) => unimplemented!(),
    }
}

/// Closes an encrypted partition.
pub(crate) fn cryptsetup_close(device: &Path) -> io::Result<()> {
    let args = &["close".into(), device.into()];
    exec("cryptsetup", None, Some(&[4]), args)
}

pub(crate) fn deactivate_volumes(volume_group: &str) -> io::Result<()> {
    let args = &["-ffyan".into(), volume_group.into()];
    exec("vgchange", None, None, args)
}

pub(crate) fn pvremove(physical_volume: &Path) -> io::Result<()> {
    let args = &["-ffy".into(), physical_volume.into()];
    exec("pvremove", None, None, args)
}

pub(crate) fn lvs(vg: &str) -> io::Result<Vec<PathBuf>> {
    info!("libdistinst: obtaining logical volumes on {}", vg);
    let mut current_line = String::with_capacity(128);
    let mut output = Vec::new();

    let mut reader = BufReader::new(
        Command::new("lvs")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .unwrap(),
    );

    // Skip the first line of output
    let _ = reader.read_line(&mut current_line);
    current_line.clear();

    while reader.read_line(&mut current_line)? != 0 {
        {
            let line = &current_line[2..];
            match line.find(' ') {
                Some(pos) => output.push(PathBuf::from(
                    ["/dev/mapper/", vg, "-", &line[..pos]].concat(),
                )),
                None => (),
            }
        }

        current_line.clear();
    }

    Ok(output)
}

pub(crate) fn pvs() -> io::Result<BTreeMap<PathBuf, Option<String>>> {
    info!("libdistinst: obtaining BTreeMap<PV, VG>");
    let mut current_line = String::with_capacity(64);
    let mut output = BTreeMap::new();

    let mut reader = BufReader::new(
        Command::new("pvs")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .unwrap(),
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

// TODO: Don't require this
struct ExternalMount<'a> {
    dest: &'a Path,
}

impl<'a> ExternalMount<'a> {
    fn new(src: &'a Path, dest: &'a Path) -> io::Result<ExternalMount<'a>> {
        exec("mount", None, None, &[src.into(), dest.into()]).map(|_| ExternalMount { dest })
    }
}

impl<'a> Drop for ExternalMount<'a> {
    fn drop(&mut self) { let _ = exec("umount", None, None, &["-l".into(), self.dest.into()]); }
}
