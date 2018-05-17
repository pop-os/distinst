//! User account information will be collected here.

use super::super::FileSystemType;
use super::{ReinstallError, mount_and_then};
use std::path::Path;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::collections::HashMap;

use misc::read;

#[derive(Default, Debug)]
pub struct AccountFiles {
    passwd: HashMap<Vec<u8>, Vec<u8>>,
    group:  HashMap<Vec<u8>, Vec<u8>>,
    shadow: HashMap<Vec<u8>, Vec<u8>>,
    gshadow: HashMap<Vec<u8>, Vec<u8>>
}

fn account(input: &[u8]) -> Vec<u8> {
    input.iter().position(|&b| b == b':').map(|position| {
            let (account, _) = input.split_at(position);
            account.to_owned()
    }).unwrap_or_else(|| Vec::new())
}

fn lines(input: &[u8]) -> HashMap<Vec<u8>, Vec<u8>> {
    input.split(|&b| b == b'\n')
        .map(|x| (account(x), x.to_owned()))
        .collect()
}

impl AccountFiles {
    pub fn new(device: &Path, fs: FileSystemType) -> Result<AccountFiles, ReinstallError> {
        info!("libdistinst: retrieving user account data");
        mount_and_then(device, fs, |base| {
            read(base.join("etc/passwd"))
                .and_then(|p| read(base.join("etc/group")).map(|g| (p, g)))
                .and_then(|(p, g)| read(base.join("etc/shadow")).map(|s| (p, g, s)))
                .and_then(|(p, g, s)| read(base.join("etc/gshadow")).map(|gs| (p, g, s, gs)))
                .map(|(ref passwd, ref group, ref shadow, ref gshadow)| AccountFiles {
                    passwd: lines(passwd),
                    group: lines(group),
                    shadow: lines(shadow),
                    gshadow: lines(gshadow)
                }).map_err(|why| ReinstallError::AccountsObtain { why, step: "get" })
        })
    }

    pub fn get(&self, home: &OsStr) -> Option<UserData> {
        let mut home_path = b"/home/".to_vec();
        home_path.extend_from_slice(home.as_bytes());
        let home: &[u8] = &home_path;

        let (user, passwd) = self.passwd.iter()
            .find(|(_, value)| get_passwd_home(value) == home)?;

        info!(
            "libdistinst: found user '{}' from home path at {}",
            String::from_utf8_lossy(&user),
            String::from_utf8_lossy(home)
        );

        let user: &[u8] = &user;
        self.group.get(user)
            .and_then(|g| self.shadow.get(user).map(|s| (g, s)))
            .and_then(|(g, s)| self.gshadow.get(user).map(|gs| (g, s, gs)))
            .map(|(group, shadow, gshadow)| {
                UserData { passwd, group, shadow, gshadow }
            })
    }
}

fn get_passwd_home(entry: &[u8]) -> &[u8] {
    entry.split(|&x| x == b':')
        .skip(5)
        .next()
        .unwrap_or(b"")
}

/// Information about a user that should be carried over to the corresponding files.
pub struct UserData<'a> {
    pub passwd: &'a [u8],
    pub shadow: &'a [u8],
    pub group: &'a [u8],
    pub gshadow: &'a [u8],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_from_passwd() {
        assert_eq!(
            get_passwd_home(b"bin:x:2:2:bin:/bin:/usr/sbin/nologin"),
            b"/bin"
        )
    }
}
