//! Serial numbers can be used to ensure that any partitions written to disks
//! are written to the correct drives, as it could be possible, however
//! unlikely, that a user could hot swap drives after obtaining device
//! information, but before writing their changes to the disk.

use std::io;
use std::path::Path;
use std::process::Command;

const PATTERN: &'static str = "E: ID_SERIAL=";

/// Obtains the serial number of the given device by calling out to `udevadm`.
///
/// The `path` should be a value like `/dev/sda`.
pub fn get_serial_no(path: &Path) -> io::Result<String> {
    Command::new("udevadm")
        .args(&["info", "--query=all", &format!("--name={}", path.display())])
        .output()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .find(|line| line.starts_with(PATTERN))
                .ok_or(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "no serial field",
                ))
                .map(|serial| serial.split_at(PATTERN.len()).1.into())
        })
}
