use disk::mount::{Mount, BIND};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::sleep;
use std::time::Duration;

/// Defines the location where a `chroot` will be performed, as well as storing
/// handles to all of the binding mounts that the chroot requires.
pub struct Chroot {
    path:       PathBuf,
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
        // Ensure that localectl writes to the chroot, instead.
        let _etc_mount = Mount::new(&self.path.join("etc"), "/etc", "none", BIND, None)?;

        let mut command = Command::new("chroot");
        command.arg(&self.path);
        command.arg(cmd.as_ref());
        command.args(args);
        command.stderr(Stdio::piped());
        command.stdout(Stdio::piped());

        debug!("{:?}", command);

        let mut child = command.spawn()?;

        // Raw pointers to child FDs, to work around borrowck.
        let stdout = child.stdout.as_mut().unwrap() as *mut _;
        let stderr = child.stderr.as_mut().unwrap() as *mut _;

        // Buffers for the child FDs, to buffer by line.
        let stdout = &mut BufReader::new(unsafe { &mut *stdout });
        let stderr = &mut BufReader::new(unsafe { &mut *stderr });

        // Buffer for reading each line from the `BufReader`s
        let buffer = &mut String::with_capacity(8 * 1024);

        loop {
            match child.try_wait()? {
                // The child has been reaped if it has an exit status.
                Some(c) => break Ok(c),
                // Pipe any output to logs that may be available.
                None => {
                    let mut finished = 0;
                    loop {
                        buffer.clear();
                         match stdout.read_line(buffer) {
                            Ok(0) | Err(_) => finished |= 1,
                            Ok(_) => {
                                info!("{}", buffer.trim());
                            }
                        }

                        buffer.clear();
                        match stderr.read_line(buffer) {
                            Ok(0) | Err(_) => finished |= 2,
                            Ok(_) => {
                                warn!("{}", buffer.trim());
                            }
                        }

                        if finished == 3 {
                            break
                        } else {
                            finished = 0;
                        }
                    }

                    sleep(Duration::from_millis(1));
                }
            }
        }
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
