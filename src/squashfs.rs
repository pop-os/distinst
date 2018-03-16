extern crate libc;

use self::libc::{pipe2, O_CLOEXEC, O_DIRECT};

use std::fs::File;
use std::io::{Error, ErrorKind, Read, Result};
use std::os::unix::io::FromRawFd;
use std::path::Path;
use std::process::{Command, Stdio};
use std::str;

enum ExtractFormat {
    Tar,
    Squashfs,
}

/// Extracts an image using either unsquashfs or tar.
pub fn extract<P: AsRef<Path>, Q: AsRef<Path>, F: FnMut(i32)>(
    archive: P,
    directory: Q,
    mut callback: F,
) -> Result<()> {
    let archive = archive.as_ref().canonicalize()?;
    let directory = directory.as_ref().canonicalize()?;

    let mut fds = [0; 2];
    let res = unsafe { pipe2(fds.as_mut_ptr(), O_CLOEXEC | O_DIRECT) };
    if res != 0 {
        return Err(Error::last_os_error());
    }

    let mut input = unsafe { File::from_raw_fd(fds[0]) };
    let output = unsafe { Stdio::from_raw_fd(fds[1]) };

    let directory = directory
        .to_str()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid directory path"))?
        .replace("'", "'\"'\"'");

    let format = if archive.extension().map_or(false, |ext| ext == "squashfs") {
        ExtractFormat::Squashfs
    } else {
        ExtractFormat::Tar
    };

    let archive = archive
        .to_str()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid archive path"))?
        .replace("'", "'\"'\"'");

    let command = match format {
        ExtractFormat::Squashfs => format!("unsquashfs -f -d '{}' '{}'", directory, archive),
        ExtractFormat::Tar => format!("tar --overwrite -xf '{}' -C '{}'", archive, directory),
    };

    debug!("{}", command);

    let mut child = Command::new("script")
        .arg("--return")
        .arg("--flush")
        .arg("--quiet")
        .arg("--command")
        .arg(command)
        .arg("/dev/null")
        .stdout(output)
        .stderr(Stdio::piped())
        .spawn()?;

    let mut last_progress = 0;
    loop {
        let mut data = [0; 0x1000];
        let count = input.read(&mut data)?;
        if count == 0 {
            break;
        }
        if let Ok(string) = str::from_utf8(&data[..count]) {
            for line in string.split(|c| c == '\r' || c == '\n') {
                let len = line.len();
                if line.starts_with('[') && line.ends_with('%') && len >= 4 {
                    if let Ok(progress) = line[len - 4..len - 1].trim().parse::<i32>() {
                        if last_progress != progress {
                            callback(progress);
                            last_progress = progress;
                        }
                    }
                }
            }
        }
    }

    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("archive extraction failed with status: {}", status),
        ))
    }
}
