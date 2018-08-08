//! User account information will be collected here.

use super::super::FileSystemType;
use super::{mount_and_then, ReinstallError};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use misc::read;

#[derive(Default, Debug)]
pub struct AccountFiles {
    passwd:  HashMap<Vec<u8>, Vec<u8>>,
    group:   HashMap<Vec<u8>, Vec<u8>>,
    shadow:  HashMap<Vec<u8>, Vec<u8>>,
    gshadow: HashMap<Vec<u8>, Vec<u8>>,
}

fn account(input: &[u8]) -> Vec<u8> {
    input
        .iter()
        .position(|&b| b == b':')
        .map(|position| {
            let (account, _) = input.split_at(position);
            account.to_owned()
        })
        .unwrap_or_else(Vec::new)
}

fn lines(input: &[u8]) -> HashMap<Vec<u8>, Vec<u8>> {
    input
        .split(|&b| b == b'\n')
        .map(|x| (account(x), x.to_owned()))
        .collect()
}

impl AccountFiles {
    pub fn new(device: &Path, fs: FileSystemType) -> Result<AccountFiles, ReinstallError> {
        info!("retrieving user account data");
        mount_and_then(device, fs, |base| {
            read(base.join("etc/passwd"))
                .and_then(|p| read(base.join("etc/group")).map(|g| (p, g)))
                .and_then(|(p, g)| read(base.join("etc/shadow")).map(|s| (p, g, s)))
                .and_then(|(p, g, s)| read(base.join("etc/gshadow")).map(|gs| (p, g, s, gs)))
                .map(
                    |(ref passwd, ref group, ref shadow, ref gshadow)| AccountFiles {
                        passwd:  lines(passwd),
                        group:   lines(group),
                        shadow:  lines(shadow),
                        gshadow: lines(gshadow),
                    },
                )
                .map_err(|why| ReinstallError::AccountsObtain { why, step: "get" })
        })
    }

    pub fn get(&self, home: &OsStr) -> Option<UserData> {
        let mut home_path = b"/home/".to_vec();
        home_path.extend_from_slice(home.as_bytes());
        let home: &[u8] = &home_path;

        let mut user_fields = None;

        for (key, passwd) in &self.passwd {
            let (group, home_field) = get_passwd_home_and_group(passwd);
            if home_field == home {
                user_fields = Some((key, group, home_field, passwd));
                break;
            }
        }

        user_fields.and_then(|(user, group_id, home, passwd)| {
            let user_string = String::from_utf8_lossy(&user);
            info!(
                "found user '{}' from home path at {}",
                user_string,
                String::from_utf8_lossy(home)
            );

            let user: &[u8] = &user;
            self.group
                .iter()
                .find(|&(_, value)| group_has_id(&value, group_id))
                .map(|(group, _)| {
                    info!(
                        "found group '{}' associated with '{}'",
                        user_string,
                        String::from_utf8_lossy(group)
                    );
                    group
                })
                .and_then(|g| self.shadow.get(user).map(|s| (g, s)))
                .and_then(|(g, s)| self.gshadow.get(user).map(|gs| (g, s, gs)))
                .map(|(group, shadow, gshadow)| UserData {
                    passwd,
                    group,
                    shadow,
                    gshadow,
                })
        })
    }
}

fn group_has_id(entry: &[u8], id: &[u8]) -> bool {
    entry
        .split(|&x| x == b':')
        .nth(2)
        .map_or(false, |field| field == id)
}

fn get_passwd_home_and_group(entry: &[u8]) -> (&[u8], &[u8]) {
    let fields = &mut entry.split(|&x| x == b':');
    let group = fields.nth(3);
    let home = fields.nth(1);

    group
        .and_then(|group| home.map(|home| (group, home)))
        .unwrap_or((b"", b""))
}

/// Information about a user that should be carried over to the corresponding files.
pub struct UserData<'a> {
    pub passwd:  &'a [u8],
    pub shadow:  &'a [u8],
    pub group:   &'a [u8],
    pub gshadow: &'a [u8],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_from_passwd() {
        assert_eq!(
            get_passwd_home_and_group(b"bin:x:2:3:bin:/bin:/usr/sbin/nologin"),
            (&b"3"[..], &b"/bin"[..])
        )
    }

    #[test]
    fn group_from_id() {
        assert!(group_has_id(b"nogroup:x:65534:", b"65534"));
        assert!(!group_has_id(b"nogroup:x:65534:", b"1"));
    }
}
