use std::io::{Error, ErrorKind, Result};
use std::path::Path;
use std::process::{Command, Stdio};

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

    debug!("{:?}", command);

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
