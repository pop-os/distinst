//! Retain users when reinstalling, keeping their home folder and user account.

use super::super::{Bootloader, Disks, FileSystemType};
use super::{mount_and_then, AccountFiles, ReinstallError, UserData};
use misc;

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

/// Removes all files in the chroot at `/`, except for `/home`.
pub fn remove_root(
    root_path: &Path,
    root_fs: FileSystemType,
) -> Result<(), ReinstallError> {
    info!("removing all files except /home. This may take a while...");
    mount_and_then(root_path, root_fs, |base| {
        read_and_exclude(base, &[OsStr::new("home")], |entry| {
            if entry.is_dir() {
                fs::remove_dir_all(entry)?;
            } else {
                fs::remove_file(entry)?;
            }

            Ok(())
        })
    })
}

/// Migrate the original system to the `/linux.old/` directory, excluding `/home`.
pub fn move_root(
    root_path: &Path,
    root_fs: FileSystemType,
) -> Result<(), ReinstallError> {
    mount_and_then(root_path, root_fs, |base| {
        let old_root = base.join("linux.old");

        // Remove an old, old root if it already exists.
        if old_root.exists () {
            info!("removing original /linux.old directory. This may take a while...");
            fs::remove_dir_all(&old_root)?;
        }

        info!("moving original system to /linux.old");
        fs::create_dir(&old_root)?;

        // Migrate the current root system to the old root path.
        let exclude = &[OsStr::new("home"), OsStr::new("linux.old")];
        read_and_exclude(base, exclude, |entry| {
            let filename = entry.file_name().expect("root entry without file name");
            fs::rename(entry, base.join("linux.old").join(filename))?;
            Ok(())
        })
    })
}

/// If a refresh install fails, this can be used to restore the original system.
pub fn recover_root(root_path: &Path, root_fs: FileSystemType) -> Result<(), ReinstallError> {
    info!("attempting to restore the original system");
    mount_and_then(root_path, root_fs, |base| {
        // Remove files installed by the installer.
        read_and_exclude(base, &[OsStr::new("home"), OsStr::new("linux.old")], |entry| {
            if entry.is_dir() {
                fs::remove_dir_all(entry)?;
            } else {
                fs::remove_file(entry)?;
            }

            Ok(())
        })?;

        // Restore original files.
        let old_root = base.join("linux.old");
        read_and_exclude(&old_root, &[], |entry| {
            let filename = entry.file_name().expect("root entry without file name");
            fs::rename(entry, base.join(filename))?;
            Ok(())
        })
    })
}

/// Delete the /linux.old directory withint the given device.
pub fn delete_old_install(root_path: &Path, root_fs: FileSystemType) -> Result<(), ReinstallError> {
    info!("removing the /linux.old directory at {:?}. This may take a while...", root_path);
    mount_and_then(root_path, root_fs, |base| {
        let old_root = base.join("linux.old");

        // Remove an old, old root if it already exists.
        if old_root.exists () {
            fs::remove_dir_all(&old_root)?;
        }

        Ok(())
    })
}

/// Checks to see if the backup install has a chance to succeed, before starting it.
pub fn validate_backup_conditions<P: AsRef<Path>>(disks: &Disks, path: P) -> Result<(), ReinstallError> {
    partition_configuration_is_valid(&disks)
        .and_then(|_| install_media_exists(path.as_ref()))
}

fn partition_configuration_is_valid(disks: &Disks) -> Result<(), ReinstallError> {
    disks.verify_partitions(Bootloader::detect())
        .map_err(|why| ReinstallError::InvalidPartitionConfiguration { why })
}

fn install_media_exists(path: &Path) -> Result<(), ReinstallError> {
    if path.exists() {
        Ok(())
    } else {
        Err(ReinstallError::MissingSquashfs {
            path: path.to_path_buf(),
        })
    }
}

fn read_and_exclude<F: FnMut(&Path) -> Result<(), ReinstallError>>(
    path: &Path,
    exclude: &[&OsStr],
    mut func: F
) -> Result<(), ReinstallError> {
    for entry in path.read_dir()? {
        if let Ok(entry) = entry {
            let entry = entry.path();
            if let Some(filename) = entry.file_name() {
                if exclude.contains(&filename) {
                    continue;
                }
            }

            func(&entry)?;
        }
    }

    Ok(())
}

pub struct Backup<'a> {
    pub users:     Vec<UserData<'a>>,
    pub localtime: Option<PathBuf>,
    pub timezone:  Option<Vec<u8>>,
    pub networks:  Option<Vec<(OsString, Vec<u8>)>>,
}

impl<'a> Backup<'a> {
    pub fn new(
        device: &Path,
        fs: FileSystemType,
        is_root: bool,
        account_files: &'a AccountFiles,
    ) -> Result<Backup<'a>, ReinstallError> {
        mount_and_then(device, fs, |base| {
            info!("collecting list of user accounts");
            let dir = if is_root {
                base.join("home").read_dir()
            } else {
                base.read_dir()
            };

            let users = dir?.filter_map(|entry| entry.ok())
                .map(|name| name.file_name())
                .inspect(|name| {
                    info!(
                        "found user account: {}",
                        name.clone().into_string().unwrap()
                    )
                })
                .collect::<Vec<OsString>>();

            info!("retaining /etc/localtime");
            let localtime = base.join("etc/localtime");
            let localtime = if localtime.exists() {
                localtime.canonicalize().ok().and_then(|ref p| get_timezone_path(p))
            } else {
                None
            };

            info!("retaining /etc/timezone");
            let timezone = base.join("etc/timezone");
            let timezone = if timezone.exists() {
                misc::read(&timezone).ok()
            } else {
                None
            };

            info!("retaining /etc/NetworkManager/system-connections/");
            let networks = base
                .join("etc/NetworkManager/system-connections/")
                .read_dir()
                .ok()
                .map(|directory| {
                    directory
                        .flat_map(|entry| entry.ok())
                        .filter(|entry| entry.path().is_file())
                        .filter_map(|conn| {
                            misc::read(conn.path())
                                .ok()
                                .map(|data| (conn.file_name(), data))
                        })
                        .collect::<Vec<(OsString, Vec<u8>)>>()
                });

            let users = users
                .iter()
                .filter_map(|user| account_files.get(user))
                .collect::<Vec<_>>();

            Ok(Backup {
                users,
                localtime,
                timezone,
                networks,
            })
        })
    }

    pub fn restore(&self, device: &Path, fs: FileSystemType) -> Result<(), ReinstallError> {
        mount_and_then(device, fs, |base| {
            info!("appending user account data to new install");
            let (passwd, group, shadow, gshadow) = (
                base.join("etc/passwd"),
                base.join("etc/group"),
                base.join("etc/shadow"),
                base.join("etc/gshadow"),
            );

            let (mut passwd, mut group, mut shadow, mut gshadow) = open(&passwd, true)
                .and_then(|p| open(&group, false).map(|g| (p, g)))
                .and_then(|(p, g)| open(&shadow, true).map(|s| (p, g, s)))
                .and_then(|(p, g, s)| open(&gshadow, true).map(|gs| (p, g, s, gs)))
                .map_err(|why| ReinstallError::AccountsObtain {
                    why,
                    step: "append",
                })?;

            group.seek(SeekFrom::End(0))?;

            fn append(entry: &[u8]) -> Vec<u8> {
                let mut entry = entry.to_owned();
                entry.push(b'\n');
                entry
            }

            for user in &self.users {
                let _ = passwd.write_all(&append(user.passwd));
                let _ = group.write_all(&append(user.group));
                let _ = shadow.write_all(&append(user.shadow));
                let _ = gshadow.write_all(&append(user.gshadow));

                if ! user.secondary_groups.is_empty() {
                    group.seek(SeekFrom::Start(0))?;
                    let groups_data = {
                        let mut buffer = Vec::with_capacity(group.metadata().ok().map_or(0, |x| x.len()) as usize);
                        group.read_to_end(&mut buffer)?;
                        buffer
                    };

                    let mut groups = super::accounts::lines::<Vec<(Vec<u8>, Vec<u8>)>>(&groups_data);
                    for &group in &user.secondary_groups {
                        for entry in &mut groups {
                            if entry.0.as_slice() == group {
                                entry.1.push(b',');
                            }
                            entry.1.extend_from_slice(group);
                        }
                    }

                    let serialized = groups.into_iter()
                        .map(|(_, entry)| entry)
                        .fold(Vec::new(), |mut acc, entry| {
                            acc.extend_from_slice(&entry);
                            acc.push(b'\n');
                            acc
                        });

                    group.seek(SeekFrom::Start(0))?;
                    group.write_all(&serialized)?;
                }
            }

            if let Some(ref tz) = self.localtime {
                info!("restoring /etc/localtime symlink to {:?}", tz);
                let path = base.join("etc/localtime");
                if path.exists() {
                    fs::remove_file(&path)?;
                }

                symlink(Path::new(tz), path)?;
            }

            if let Some(ref tz) = self.timezone {
                info!("restoring /etc/timezone with {}", String::from_utf8_lossy(tz));
                misc::create(base.join("etc/timezone")).and_then(|mut file| file.write_all(tz))?;
            }

            if let Some(ref networks) = self.networks {
                info!("restoring NetworkManager configuration");
                let network_conf_dir = &base.join("etc/NetworkManager/system-connections/");
                let _ = fs::create_dir_all(&network_conf_dir);

                for &(ref connection, ref data) in networks {
                    create_network_conf(network_conf_dir, connection, data);
                }
            }

            Ok(())
        })
    }
}

fn create_network_conf(base: &Path, conn: &OsStr, data: &[u8]) {
    let result = misc::create(base.join(conn)).and_then(|mut file| {
        file.write_all(data)
            .and_then(|_| file.set_permissions(Permissions::from_mode(0o600)))
    });

    if let Err(why) = result {
        warn!("failed to write network configuration file: {}", why);
    }
}

fn open(path: &Path, append: bool) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).append(append).open(path)
}

fn get_timezone_path(tz: &Path) -> Option<PathBuf> {
    let raw = tz.as_os_str().as_bytes();
    const PATTERN: &[u8] = b"zoneinfo/";
    const PREFIX: &[u8] = b"../usr/share/zoneinfo/";

    raw.windows(PATTERN.len())
        .rposition(|window| window == PATTERN)
        .and_then(|position| {
            let (_, tz) = raw.split_at(position + PATTERN.len());
            let mut vec = PREFIX.to_vec();
            vec.extend_from_slice(tz);
            String::from_utf8(vec).ok()
        })
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn localtime() {
        assert_eq!(
            get_timezone_path(Path::new(
                "/tmp/prefix.id/usr/share/zoneinfo/America/Denver"
            )),
            Some(PathBuf::from("../usr/share/zoneinfo/America/Denver"))
        )
    }
}
