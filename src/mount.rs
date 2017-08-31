use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub enum MountKind {
    Bind,
}

pub struct Mount {
    source: PathBuf,
    dest: PathBuf,
    kind: MountKind,
    mounted: bool,
}

impl Mount {
    pub fn new<P: AsRef<Path>, Q: AsRef<Path>>(source: P, dest: Q, kind: MountKind) -> Result<Mount> {
        let source = source.as_ref().canonicalize()?;
        let dest = dest.as_ref().canonicalize()?;

        println!("Mounting {}", dest.display());

        let mut command = Command::new("mount");
        match kind {
            MountKind::Bind => {
                command.arg("--bind");
            }
        }
        command.arg(&source);
        command.arg(&dest);

        let status = command.status()?;
        if status.success() {
            Ok(Mount {
                source: source,
                dest: dest,
                kind: kind,
                mounted: true,
            })
        } else {
            Err(Error::new(
                ErrorKind::PermissionDenied,
                format!("mount failed with status: {}", status)
            ))
        }
    }

    pub fn unmount(&mut self, lazy: bool) -> Result<()> {
        if self.mounted {
            println!("Unmounting {}", self.dest.display());

            let mut command = Command::new("umount");
            if lazy {
                command.arg("--lazy");
            }
            command.arg(&self.dest);

            let status = command.status()?;
            if status.success() {
                self.mounted = false;
                Ok(())
            } else {
                Err(Error::new(
                    ErrorKind::PermissionDenied,
                    format!("unmount failed with status: {}", status)
                ))
            }
        } else {
            Ok(())
        }
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        println!("Mount drop");
        let _ = self.unmount(true);
    }
}
