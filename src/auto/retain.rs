//! Retain users when reinstalling, keeping their home folder and user account.

use super::super::{Bootloader, Disks, FileSystemType};
use super::{ReinstallError, UserData, mount_and_then};

use std::path::Path;
use std::io::{self, Write};
use std::fs::{self, File, OpenOptions};
use std::ffi::{OsStr, OsString};

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

pub fn get_users_on_device(
    device: &Path,
    fs: FileSystemType,
    is_root: bool
) -> Result<Vec<OsString>, ReinstallError> {
    info!("libdistinst: collecting list of user accounts");
    mount_and_then(device, fs, |base| {
        let dir = if is_root {
            base.join("home").read_dir()
        } else {
            base.read_dir()
        };

        dir.map_err(|why| ReinstallError::IO { why })
            .map(|dir| {
                dir.filter_map(|entry| entry.ok())
                    .map(|name| name.file_name())
                    .inspect(|name| info!(
                        "libdistinst: backing up {}",
                        name.clone().into_string().unwrap()
                    )).collect::<Vec<OsString>>()
            })
    })
}

pub fn add_users_on_device(
    device: &Path,
    fs: FileSystemType,
    user_data: &[UserData]
) -> Result<(), ReinstallError> {
    info!("libdistinst: appending user account data to new install");
    mount_and_then(device, fs, |base| {
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

        Ok(())
    })
}

fn open(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).append(true).open(path)
}
