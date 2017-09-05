use std::ffi::OsStr;
use std::io::{Error, ErrorKind, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn parted<P: AsRef<Path>, S: AsRef<OsStr>, I: IntoIterator<Item=S>>(disk: P, args: I) -> Result<()> {
    let mut command = Command::new("parted");

    command.arg("-s");
    command.arg("--align");
    command.arg("optimal");
    command.arg(disk.as_ref());

    for arg in args {
        command.arg(arg);
    }

    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("parted failed with status: {}", status)
        ))
    }
}

pub fn partx<P: AsRef<Path>>(disk: P) -> Result<()> {
    let mut command = Command::new("partx");

    command.arg("-u");
    command.arg(disk.as_ref());

    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("partx failed with status: {}", status)
        ))
    }
}

#[derive(Copy, Clone, Debug)]
pub enum MkfsKind {
    Fat32,
    Ext4,
}

pub fn mkfs<P: AsRef<Path>>(part: P, kind: MkfsKind) -> Result<()> {
    let mut command = match kind {
        MkfsKind::Fat32 => {
            let mut command = Command::new("mkfs.fat");

            command.arg("-F");
            command.arg("32");
            command.arg(part.as_ref());

            command.stdout(Stdio::null());

            command
        },
        MkfsKind::Ext4 => {
            let mut command = Command::new("mkfs.ext4");

            command.arg("-F");
            command.arg("-q");
            command.arg(part.as_ref());

            command.stdout(Stdio::null());

            command
        }
    };

    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("mkfs for {:?} failed with status: {}", kind, status)
        ))
    }
}
