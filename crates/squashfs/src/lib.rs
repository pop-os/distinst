extern crate libc;
#[macro_use]
extern crate log;

use std::{
    fs::File,
    io::{Error, ErrorKind, Read, Result},
    os::unix::{
        io::{AsRawFd, FromRawFd, RawFd},
        process::CommandExt,
    },
    path::Path,
    process::{Command, Stdio},
    str,
};

fn getpty(columns: u32, lines: u32) -> (RawFd, String) {
    use std::{
        ffi::CStr,
        fs::OpenOptions,
        io,
        os::unix::{fs::OpenOptionsExt, io::IntoRawFd},
    };

    extern "C" {
        fn ptsname(fd: libc::c_int) -> *const libc::c_char;
        fn grantpt(fd: libc::c_int) -> libc::c_int;
        fn unlockpt(fd: libc::c_int) -> libc::c_int;
    }

    let master_fd = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC)
        .open("/dev/ptmx")
        .unwrap()
        .into_raw_fd();
    unsafe {
        if grantpt(master_fd) < 0 {
            panic!("grantpt: {:?}", Error::last_os_error());
        }
        if unlockpt(master_fd) < 0 {
            panic!("unlockpt: {:?}", Error::last_os_error());
        }
    }

    unsafe {
        let size = libc::winsize {
            ws_row:    lines as libc::c_ushort,
            ws_col:    columns as libc::c_ushort,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        if libc::ioctl(master_fd, libc::TIOCSWINSZ, &size as *const libc::winsize) < 0 {
            panic!("ioctl: {:?}", io::Error::last_os_error());
        }
    }

    let tty_path = unsafe { CStr::from_ptr(ptsname(master_fd)).to_string_lossy().into_owned() };
    (master_fd, tty_path)
}

fn slave_stdio(tty_path: &str) -> Result<(File, File, File)> {
    use libc::{O_CLOEXEC, O_RDONLY, O_WRONLY};
    use std::ffi::CString;

    let cvt = |res: i32| -> Result<i32> {
        if res < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(res)
        }
    };

    let tty_c = CString::new(tty_path).unwrap();
    let stdin =
        unsafe { File::from_raw_fd(cvt(libc::open(tty_c.as_ptr(), O_CLOEXEC | O_RDONLY))?) };
    let stdout =
        unsafe { File::from_raw_fd(cvt(libc::open(tty_c.as_ptr(), O_CLOEXEC | O_WRONLY))?) };
    let stderr =
        unsafe { File::from_raw_fd(cvt(libc::open(tty_c.as_ptr(), O_CLOEXEC | O_WRONLY))?) };

    Ok((stdin, stdout, stderr))
}

fn before_exec() -> Result<()> {
    unsafe {
        if libc::setsid() < 0 {
            panic!("setsid: {:?}", Error::last_os_error());
        }
        if libc::ioctl(0, libc::TIOCSCTTY, 1) < 0 {
            panic!("ioctl: {:?}", Error::last_os_error());
        }
    }

    Ok(())
}

fn handle<F: FnMut(i32)>(mut master: File, mut callback: F) -> Result<()> {
    let mut last_progress = 0;
    loop {
        let mut data = [0; 0x1000];
        let count = master.read(&mut data)?;
        if count == 0 {
            return Ok(());
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
}

enum ExtractFormat {
    Tar,
    Squashfs,
}

/// Extracts an image using either unsquashfs or tar.
pub fn extract<P: AsRef<Path>, Q: AsRef<Path>, F: FnMut(i32)>(
    archive: P,
    directory: Q,
    callback: F,
) -> Result<()> {
    let archive = archive.as_ref().canonicalize()?;
    let directory = directory.as_ref().canonicalize()?;

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

    let mut command = match format {
        ExtractFormat::Squashfs => {
            let mut command = Command::new("unsquashfs");
            command.arg("-f").arg("-d").arg(directory).arg(archive);
            command
        }
        ExtractFormat::Tar => {
            let mut command = Command::new("tar");
            command.arg("--overwrite").arg("-xf").arg(archive).arg("-C").arg(directory);
            command
        }
    };

    debug!("{:?}", command);

    let (master_fd, tty_path) = getpty(80, 30);
    let mut child = {
        let (slave_stdin, slave_stdout, slave_stderr) = slave_stdio(&tty_path)?;

        unsafe {
            command
                .stdin(Stdio::from_raw_fd(slave_stdin.as_raw_fd()))
                .stdout(Stdio::from_raw_fd(slave_stdout.as_raw_fd()))
                .stderr(Stdio::from_raw_fd(slave_stderr.as_raw_fd()))
                .env("COLUMNS", "")
                .env("LINES", "")
                .env("TERM", "xterm-256color")
                .pre_exec(before_exec)
                .spawn()?
        }
    };

    let master = unsafe { File::from_raw_fd(master_fd) };
    match handle(master, callback) {
        Ok(()) => (),
        Err(err) => match err.raw_os_error() {
            // EIO happens when slave end is closed
            Some(libc::EIO) => (),
            // Log other errors, use status code below to return
            _ => error!("handle error: {}", err),
        },
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
