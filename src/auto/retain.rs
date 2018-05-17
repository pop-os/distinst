//! Retain users when reinstalling, keeping their home folder and user account.

use super::super::{Bootloader, Disks, FileSystemType};
use super::{ReinstallError, UserData, mount_and_then};
use misc;

use std::path::{Path, PathBuf};
use std::io::{self, Write};
use std::fs::{self, File, OpenOptions};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;

pub fn validate_before_removing<P: AsRef<Path>>(
    disks: &Disks,
    path: P,
    home_path: &Path,
    home_fs: FileSystemType
) -> Result<(), ReinstallError> {
    partition_configuration_is_valid(&disks)
        .and_then(|_| install_media_exists(path.as_ref()))
        .and_then(|_| remove_all_except(home_path, home_fs, &[OsStr::new("home")]))
}

fn partition_configuration_is_valid(disks: &Disks) -> Result<(), ReinstallError> {
    disks.verify_partitions(Bootloader::detect())
        .map_err(|why| ReinstallError::InvalidPartitionConfiguration { why })
}

fn install_media_exists(path: &Path) -> Result<(), ReinstallError> {
    if path.exists() {
        Ok(())
    } else {
        Err(ReinstallError::MissingSquashfs { path: path.to_path_buf() })
    }
}

fn remove_all_except(
    device: &Path,
    fs: FileSystemType,
    exclude: &[&OsStr]
) -> Result<(), ReinstallError> {
    mount_and_then(device, fs, |base| {
        for entry in base.read_dir().map_err(|why| ReinstallError::IO { why })? {
            if let Ok(entry) = entry {
                let entry = entry.path();
                if let Some(filename) = entry.file_name() {
                    if exclude.contains(&filename) {
                        continue;
                    }
                }

                if entry.is_dir() {
                    let _ = fs::remove_dir_all(entry);
                } else {
                    let _ = fs::remove_file(entry);
                }
            }
        }

        Ok(())
    })
}

pub struct Backup {
    pub users: Vec<OsString>,
    pub localtime: Option<PathBuf>,
    pub timezone: Option<Vec<u8>>,
}

pub fn get_data_on_device(
    device: &Path,
    fs: FileSystemType,
    is_root: bool
) -> Result<Backup, ReinstallError> {
    info!("libdistinst: collecting list of user accounts");
    mount_and_then(device, fs, |base| {
        let dir = if is_root {
            base.join("home").read_dir()
        } else {
            base.read_dir()
        };

        let users = dir.map_err(|why| ReinstallError::IO { why })
            .map(|dir| {
                dir.filter_map(|entry| entry.ok())
                    .map(|name| name.file_name())
                    .inspect(|name| info!(
                        "libdistinst: backing up {}",
                        name.clone().into_string().unwrap()
                    )).collect::<Vec<OsString>>()
            })?;

        let localtime = base.join("etc/localtime");
        let localtime = if localtime.exists() {
            localtime.canonicalize().ok().and_then(get_timezone_path)
        } else {
            None
        };

        let timezone = base.join("etc/timezone");
        let timezone = if timezone.exists() {
            misc::read(&timezone).ok()
        } else {
            None
        };

        Ok(Backup { users, localtime, timezone })
    })
}

pub fn add_users_on_device(
    device: &Path,
    fs: FileSystemType,
    user_data: &[UserData],
    localtime: Option<PathBuf>,
    timezone: Option<Vec<u8>>,
) -> Result<(), ReinstallError> {
    mount_and_then(device, fs, |base| {
        info!("libdistinst: appending user account data to new install");
        let (passwd, group, shadow, gshadow) = (
            base.join("etc/passwd"),
            base.join("etc/group"),
            base.join("etc/shadow"),
            base.join("etc/gshadow")
        );

        let (mut passwd, mut group, mut shadow, mut gshadow) = open(&passwd)
            .and_then(|p| open(&group).map(|g| (p, g)))
            .and_then(|(p, g)| open(&shadow).map(|s| (p, g, s)))
            .and_then(|(p, g, s)| open(&gshadow).map(|gs| (p, g, s, gs)))
            .map_err(|why| ReinstallError::AccountsObtain { why, step: "append" })?;

        fn append(entry: &[u8]) -> Vec<u8> {
            let mut entry = entry.to_owned();
            entry.push(b'\n');
            entry
        }

        for user in user_data {
            let _ = passwd.write_all(&append(user.passwd));
            let _ = group.write_all(&append(user.group));
            let _ = shadow.write_all(&append(user.shadow));
            let _ = gshadow.write_all(&append(user.gshadow));
        }

        if let Some(ref tz) = localtime {
            info!("libdistinst: restoring /etc/localtime symlink to {:?}", tz);
            symlink(base.join(tz), base.join("etc/localtime"))?;
        }

        if let Some(ref tz) = timezone {
            info!("libdistinst: restoring /etc/timezone with {}", String::from_utf8_lossy(tz));
            File::create(base.join("etc/timezone")).and_then(|mut file| file.write_all(tz))?;
        }

        Ok(())
    })
}

fn open(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).append(true).open(path)
}

fn get_timezone_path(tz: PathBuf) -> Option<PathBuf> {
    let raw = tz.as_os_str().as_bytes();
    const PATTERN: &[u8] = b"zoneinfo/";
    const PREFIX: &[u8] = b"usr/share/zoneinfo/";

    raw.windows(PATTERN.len()).rposition(|window| window == PATTERN)
        .and_then(|position| {
            let (_, tz) = raw.split_at(position + PATTERN.len());
            let mut vec = PREFIX.to_vec();
            vec.extend_from_slice(tz);
            String::from_utf8(vec).ok()
        }).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn localtime() {
        assert_eq!(
            get_timezone_path(PathBuf::from("/tmp/prefix.id/usr/share/zoneinfo/America/Denver")),
            Some(PathBuf::from("usr/share/zoneinfo/America/Denver"))
        )
    }
}
