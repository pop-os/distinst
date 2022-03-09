//! User account information will be collected here.

use super::ReinstallError;
use std::{collections::HashMap, ffi::OsStr, os::unix::ffi::OsStrExt, path::Path};

use crate::misc::read;

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

pub(crate) fn lines<T: ::std::iter::FromIterator<(Vec<u8>, Vec<u8>)>>(input: &[u8]) -> T {
    input.split(|&b| b == b'\n').map(|x| (account(x), x.to_owned())).collect::<T>()
}

impl AccountFiles {
    pub fn new(base: &Path) -> Result<AccountFiles, ReinstallError> {
        info!("retrieving user account data");
        read(base.join("etc/passwd"))
            .and_then(|p| read(base.join("etc/group")).map(|g| (p, g)))
            .and_then(|(p, g)| read(base.join("etc/shadow")).map(|s| (p, g, s)))
            .and_then(|(p, g, s)| read(base.join("etc/gshadow")).map(|gs| (p, g, s, gs)))
            .map(|(ref passwd, ref group, ref shadow, ref gshadow)| AccountFiles {
                passwd:  lines(passwd),
                group:   lines(group),
                shadow:  lines(shadow),
                gshadow: lines(gshadow),
            })
            .map_err(|why| ReinstallError::AccountsObtain { why, step: "get" })
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
            let user_string = String::from_utf8_lossy(user);
            info!(
                "found user '{}' from home path at {}",
                user_string,
                String::from_utf8_lossy(home)
            );

            let group = self.group.iter().find(|&(_, value)| group_has_id(value, group_id)).map(
                |(group, value)| {
                    info!(
                        "found group '{}' associated with '{}'",
                        user_string,
                        String::from_utf8_lossy(group)
                    );
                    value
                },
            )?;

            let secondary_groups = self
                .group
                .iter()
                .filter(|&(_, value)| group_has_user(value, user))
                .inspect(|&(group, _)| {
                    info!(
                        "{} has a secondary group: '{}'",
                        String::from_utf8_lossy(user),
                        String::from_utf8_lossy(group)
                    )
                })
                .map(|(group, _)| group.as_slice())
                .collect::<Vec<&[u8]>>();

            let shadow = self.shadow.get(user)?;
            let gshadow = self.gshadow.get(user)?;

            Some(UserData { user, passwd, group, shadow, gshadow, secondary_groups })
        })
    }
}

fn group_has_id(entry: &[u8], id: &[u8]) -> bool {
    entry.split(|&x| x == b':').nth(2).map_or(false, |field| field == id)
}

fn group_has_user(entry: &[u8], user: &[u8]) -> bool {
    entry
        .split(|&x| x == b':')
        .nth(3)
        .map_or(false, |field| field.split(|&x| x == b',').any(|f| f == user))
}

fn get_passwd_home_and_group(entry: &[u8]) -> (&[u8], &[u8]) {
    let fields = &mut entry.split(|&x| x == b':');
    let group = fields.nth(3);
    let home = fields.nth(1);

    group.and_then(|group| home.map(|home| (group, home))).unwrap_or((b"", b""))
}

/// Information about a user that should be carried over to the corresponding files.
pub struct UserData<'a> {
    pub user:             &'a [u8],
    pub passwd:           &'a [u8],
    pub shadow:           &'a [u8],
    pub group:            &'a [u8],
    pub gshadow:          &'a [u8],
    pub secondary_groups: Vec<&'a [u8]>,
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

    #[test]
    fn secondary_groups() {
        assert!(group_has_user(b"random:x:12345:user_x,user_b,user_c", b"user_b"));
        assert!(!group_has_user(b"random:x:12345:user_x,user_b,user_c", b"user_d"));
    }
}
