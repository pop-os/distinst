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

    pub fn get(&self, user: &OsStr) -> Option<UserData> {
        let user = user.as_bytes();
        self.passwd.get(user)
            .and_then(|p| self.group.get(user).map(|g| (p, g)))
            .and_then(|(p, g)| self.shadow.get(user).map(|s| (p, g, s)))
            .and_then(|(p, g, s)| self.gshadow.get(user).map(|gs| (p, g, s, gs)))
            .map(|(passwd, group, shadow, gshadow)| {
                UserData { passwd, group, shadow, gshadow }
            })
    }
}

/// Information about a user that should be carried over to the corresponding files.
pub struct UserData<'a> {
    pub passwd: &'a [u8],
    pub shadow: &'a [u8],
    pub group: &'a [u8],
    pub gshadow: &'a [u8],
}
