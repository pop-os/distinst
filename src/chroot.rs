use mount::{self, Mount, MountOption};
use std::ffi::OsStr;
use std::io::Result;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

pub struct Chroot {
    path:       PathBuf,
    dev_mount:  Mount,
    pts_mount:  Mount,
    proc_mount: Mount,
    run_mount:  Mount,
    sys_mount:  Mount,
}

impl Chroot {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Chroot> {
        let path = path.as_ref().canonicalize()?;
        let dev_mount = Mount::new("/dev", path.join("dev"), &[MountOption::Bind])?;
        let pts_mount = Mount::new(
            "/dev/pts",
            path.join("dev").join("pts"),
            &[MountOption::Bind],
        )?;
        let proc_mount = Mount::new("/proc", path.join("proc"), &[MountOption::Bind])?;
        let run_mount = Mount::new("/run", path.join("run"), &[MountOption::Bind])?;
        let sys_mount = Mount::new("/sys", path.join("sys"), &[MountOption::Bind])?;
        Ok(Chroot {
            path:       path,
            dev_mount:  dev_mount,
            pts_mount:  pts_mount,
            proc_mount: proc_mount,
            run_mount:  run_mount,
            sys_mount:  sys_mount,
        })
    }

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
