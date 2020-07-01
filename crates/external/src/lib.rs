//! A collection of external commands used throughout the program.

extern crate disk_types;
extern crate distinst_utils as misc;
#[macro_use]
extern crate log;
extern crate proc_mounts;
extern crate rand;
#[macro_use]
extern crate smart_default;
extern crate sys_mount;
extern crate tempdir;

pub mod block;
pub mod luks;
pub mod lvm;
pub(crate) mod retry;

pub use self::{block::*, luks::*, lvm::*};

use std::{
    ffi::OsString,
    io::{self, Write},
    process::{Command, Stdio},
};

/// A generic function for executing a variety of external commands.
pub fn exec(
    cmd: &str,
    stdin: Option<&[u8]>,
    valid_codes: Option<&'static [i32]>,
    args: &[OsString],
) -> io::Result<()> {
    info!("executing {} with {:?}", cmd, args);

    let mut child = Command::new(cmd)
        .args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::null())
        .spawn()?;

    if let Some(stdin) = stdin {
        child.stdin.as_mut().expect("stdin not obtained").write_all(stdin)?;
    }

    let status = child.wait()?;
    let success = status.success()
        || valid_codes
            .map_or(false, |codes| status.code().map_or(false, |code| codes.contains(&code)));

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

fn mebibytes(bytes: u64) -> String { format!("{}", bytes / (1024 * 1024)) }
