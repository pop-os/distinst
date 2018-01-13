use std::io::{Error, ErrorKind, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use super::FileSystemType;

pub fn mkfs<P: AsRef<Path>>(part: P, kind: FileSystemType) -> Result<()> {
    let mut command = match kind {
        FileSystemType::Fat32 => {
            let mut command = Command::new("mkfs.fat");

            command.arg("-F");
            command.arg("32");
            command.arg(part.as_ref());

            command.stdout(Stdio::null());

            command
        },
        FileSystemType::Ext4 => {
            let mut command = Command::new("mkfs.ext4");

            command.arg("-F");
            command.arg("-q");
            command.arg(part.as_ref());

            command.stdout(Stdio::null());

            command
        },
        _ => unimplemented!(),
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
