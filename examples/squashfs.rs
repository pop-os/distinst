extern crate libc;

use libc::{pipe2, O_CLOEXEC, O_DIRECT, O_NONBLOCK};

use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::os::unix::io::FromRawFd;
use std::process::{Command, Stdio};
use std::str;

fn main() {
    let mut fds = [0; 2];
    let res = unsafe { pipe2(fds.as_mut_ptr(), O_CLOEXEC | O_DIRECT) };
    if res != 0 {
        panic!("pipe2 failed: {}", Error::last_os_error());
    }

    let mut input = unsafe { File::from_raw_fd(fds[0]) };
    let output = unsafe { Stdio::from_raw_fd(fds[1]) };

    let mut child = Command::new("sudo")
        .arg("script").arg("--return").arg("--flush").arg("--quiet").arg("--command")
        .arg("unsquashfs -f -d /tmp/squashfs bash/filesystem.squashfs")
        .arg("/dev/null")
        .stdout(output)
        .spawn().unwrap();

    loop {
        let mut data = [0; 4096];
        let count = input.read(&mut data).unwrap();
        if let Ok(string) = str::from_utf8(&data[..count]) {
            for line in string.split(|c| c == '\r' || c == '\n') {
                let len = line.len();
                if line.starts_with('[') && line.ends_with('%') && len >= 4 {
                    if let Ok(progress) = line[len - 4..len - 1].trim().parse::<i32>() {
                        println!("\r{}%", progress);
                    }
                }
            }
        }
        if count == 0 {
            break;
        }
    }

    println!("{}", child.wait().unwrap());
}
