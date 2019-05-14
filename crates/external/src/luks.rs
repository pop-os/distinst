use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use sys_mount::*;
use proc_mounts::MountIter;

use super::*;

/// Get a vector of encrypted devices
pub fn encrypted_devices() -> io::Result<Vec<String>> {
    let mut current_line = String::with_capacity(64);
    let mut output = Vec::new();

    let mut reader = BufReader::new(
        Command::new("dmsetup")
            .args(&["ls", "--target", "crypt"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .expect("failed to execute dmsetup command"),
    );

    reader.read_line(&mut current_line)?;
    if current_line.starts_with("No devices found") {
        return Ok(Vec::new());
    }

    {
        let mut fields = current_line.split_whitespace();
        if let Some(dm) = fields.next() {
            output.push(dm.into());
        }
    }

    current_line.clear();

    while reader.read_line(&mut current_line)? != 0 {
        {
            let mut fields = current_line.split_whitespace();
            if let Some(dm) = fields.next() {
                output.push(dm.into());
            }
        }

        current_line.clear();
    }

    Ok(output)
}

/// If `cryptsetup info DEV` has an exit status of 0, the partition is encrypted.
pub fn is_encrypted(device: &Path) -> bool {
    let mut attempts = 0;
    loop {
        let res = Command::new("cryptsetup")
            .stdout(Stdio::null())
            .arg("luksDump")
            .arg(device)
            .status()
            .ok();

        match res.and_then(|stat| stat.code()) {
            Some(0) => return true,
            // An exit status of 4 can happen if the partition is scanned too hastily.
            Some(4) => {
                thread::sleep(Duration::from_millis(100));
                if attempts == 3 {
                    return false;
                }
                attempts += 1;
                continue
            },
            _ => return false,
        }
    }
}

pub enum CloseBy<'a> {
    Path(&'a Path),
    Name(&'a str),
}

/// Closes an encrypted partition.
pub fn cryptsetup_close(device: CloseBy) -> io::Result<()> {
    let args = &["close".into(), match device {
        CloseBy::Path(path) => path.into(),
        CloseBy::Name(name) => name.into(),
    }];
    exec("cryptsetup", None, Some(&[4]), args)
}

/// Deactivate all logical devies found on the system.
pub fn deactivate_logical_devices() -> io::Result<()> {
    let mut res = Ok(());

    // Unmount all mounted logical devices.
    if let Ok(mount_iterator) = MountIter::new() {
        for mount in mount_iterator.filter_map(Result::ok) {
            if mount.source.starts_with("/dev/mapper") {
                debug!("unmounting {:?}", mount.dest);
                let _ = unmount(&mount.dest, UnmountFlags::DETACH);
            }
        }
    }

    for luks_pv in encrypted_devices()? {
        info!("deactivating encrypted device named {}", luks_pv);
        if let Some(vg) = pvs()?.get(&PathBuf::from(["/dev/mapper/", &luks_pv].concat())) {
            match *vg {
                Some(ref vg) => {
                    if let Err(why) = vgdeactivate(vg).and_then(|_| cryptsetup_close(CloseBy::Name(&luks_pv))) {
                        res = Err(why);
                    }
                },
                None => {
                    if let Err(why) = cryptsetup_close(CloseBy::Name(&luks_pv)) {
                        res = Err(why);
                    }
                },
            }
        }
    }

    res
}
