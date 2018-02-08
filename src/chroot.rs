use disk::mount::{Mount, BIND};
use std::ffi::OsStr;
use std::io::Result;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

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
    /// Performs binding mounts of all required paths to ensure that a chroot is successful.
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
            path:       path,
            dev_mount:  dev_mount,
            pts_mount:  pts_mount,
            proc_mount: proc_mount,
            run_mount:  run_mount,
            sys_mount:  sys_mount,
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

        debug!("{:?}", command);

        command.status()
    }

    /// Return true if the filesystem was unmounted, false if it was already unmounted
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
