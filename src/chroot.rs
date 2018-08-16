use disk::mount::{Mount, BIND};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

/// Defines the location where a `chroot` will be performed, as well as storing
/// handles to all of the binding mounts that the chroot requires.
pub struct Chroot {
    pub path:   PathBuf,
    dev_mount:  Mount,
    pts_mount:  Mount,
    proc_mount: Mount,
    run_mount:  Mount,
    sys_mount:  Mount,
}

impl Chroot {
    /// Performs binding mounts of all required paths to ensure that a chroot
    /// is successful.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Chroot> {
        let path = path.as_ref().canonicalize()?;
        let dev_mount = Mount::new("/dev", &path.join("dev"), "none", BIND, None)?;
        let pts_mount = Mount::new(
            "/dev/pts",
            &path.join("dev").join("pts"),
            "none",
            BIND,
            None,
        )?;
        let proc_mount = Mount::new("/proc", &path.join("proc"), "none", BIND, None)?;
        let run_mount = Mount::new("/run", &path.join("run"), "none", BIND, None)?;
        let sys_mount = Mount::new("/sys", &path.join("sys"), "none", BIND, None)?;
        Ok(Chroot {
            path,
            dev_mount,
            pts_mount,
            proc_mount,
            run_mount,
            sys_mount,
        })
    }

    /// Executes an external command with `chroot`.
    pub fn command<S: AsRef<OsStr>, T: AsRef<OsStr>, I: IntoIterator<Item = T>>(
        &mut self,
        cmd: S,
        args: I,
    ) -> Result<ExitStatus> {
        let mut command = Command::new("chroot");
        command.arg(&self.path);
        command.arg(cmd.as_ref());
        command.args(args);
        command.stderr(Stdio::piped());
        command.stdout(Stdio::piped());

        debug!("{:?}", command);

        let mut child = command.spawn().map_err(|why| Error::new(
            ErrorKind::Other,
            format!("chroot command failed to spawn: {}", why)
        ))?;

        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        thread::spawn(move || {
            let buffer = &mut String::with_capacity(8 * 1024);
            loop {
                buffer.clear();
                match stdout.read_line(buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        info!("{}", buffer.trim_right());
                    }
                }
            }
        });

        let mut stderr = BufReader::new(child.stderr.take().unwrap());
        thread::spawn(move || {
            let buffer = &mut String::with_capacity(8 * 1024);
            loop {
                buffer.clear();
                match stderr.read_line(buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        warn!("{}", buffer.trim_right());
                    }
                }
            }
        });

        child.wait().map_err(|why| Error::new(
            ErrorKind::Other,
            format!("waiting on chroot child process failed: {}", why)
        ))
    }

    /// Return true if the filesystem was unmounted, false if it was already
    /// unmounted
    pub fn unmount(&mut self, lazy: bool) -> Result<()> {
        self.sys_mount.unmount(lazy)?;
        self.run_mount.unmount(lazy)?;
        self.proc_mount.unmount(lazy)?;
        self.pts_mount.unmount(lazy)?;
        self.dev_mount.unmount(lazy)?;
        Ok(())
    }
}

impl Drop for Chroot {
    fn drop(&mut self) {
        // Ensure unmounting
        let _ = self.sys_mount.unmount(true);
        let _ = self.run_mount.unmount(true);
        let _ = self.proc_mount.unmount(true);
        let _ = self.pts_mount.unmount(true);
        let _ = self.dev_mount.unmount(true);
    }
}
