use std::ffi::OsStr;
use std::io::{Error, ErrorKind, Result};
use std::path::Path;
use std::process::{Command, Stdio};

/// Utilized for ensuring that block & partition information has synced with the OS.
pub fn blockdev<P: AsRef<Path>, S: AsRef<OsStr>, I: IntoIterator<Item = S>>(
    disk: P,
    args: I,
) -> Result<()> {
    let mut command = Command::new("blockdev");

    command.args(args);
    command.arg(disk.as_ref());

    debug!("{:?}", command);

    let status = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("blockdev failed with status: {}", status),
        ))
    }
}
