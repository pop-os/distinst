//! Retain users when reinstalling, keeping their home folder and user account.

use super::super::FileSystemType;
use super::{ReinstallError, UserData, mount_and_then};

use std::path::Path;
use std::io::Write;
use std::fs::{self, File};
use std::ffi::{OsStr, OsString};

pub fn remove_all_except(
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
                    .collect::<Vec<OsString>>()
            })
    })
}

pub fn add_users_on_device(
    device: &Path,
    fs: FileSystemType,
    user_data: &[UserData]
) -> Result<(), ReinstallError> {
    mount_and_then(device, fs, |base| {
        let (mut passwd, mut group, mut shadow, mut gshadow) = File::open(base.join("etc/passwd"))
            .and_then(|p| File::open(base.join("etc/group")).map(|g| (p, g)))
            .and_then(|(p, g)| File::open(base.join("etc/shadow")).map(|s| (p, g, s)))
            .and_then(|(p, g, s)| File::open(base.join("etc/gshadow")).map(|gs| (p, g, s, gs)))
            .map_err(|why| ReinstallError::AccountsObtain { why })?;

        fn append(entry: &[u8]) -> Vec<u8> {
            let mut vec = vec![b'\n'];
            vec.extend_from_slice(entry);
            vec
        }

        for user in user_data {
            let _ = passwd.write(&append(user.passwd));
            let _ = group.write(&append(user.group));
            let _ = shadow.write(&append(user.shadow));
            let _ = gshadow.write(&append(user.gshadow));
        }

        Ok(())
    })
}
