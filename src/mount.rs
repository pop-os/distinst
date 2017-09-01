use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct Mount {
    source: PathBuf,
    dest: PathBuf,
    mounted: bool,
}

#[derive(Copy, Clone, Debug)]
pub enum MountOption {
    Bind,
    Synchronize,
}

impl Mount {
    pub fn new<P: AsRef<Path>, Q: AsRef<Path>>(source: P, dest: Q, options: &[MountOption]) -> Result<Mount> {
        let source = source.as_ref().canonicalize()?;
        let dest = dest.as_ref().canonicalize()?;

        //println!("Mounting {} to {} with {:?}", source.display(), dest.display(), options);

        let mut command = Command::new("mount");

        let mut option_strings = Vec::new();
        for &option in options.iter() {
            match option {
                MountOption::Bind => {
                    command.arg("--bind");
                },
                MountOption::Synchronize => {
                    option_strings.push("sync");
                }
            }
        }

        option_strings.sort();
        option_strings.dedup();
        if ! option_strings.is_empty() {
            command.arg("-o");
            command.arg(option_strings.join(","));
        }

        command.arg(&source);
        command.arg(&dest);

        let status = command.status()?;
        if status.success() {
            Ok(Mount {
                source: source,
                dest: dest,
                mounted: true,
            })
        } else {
            Err(Error::new(
                ErrorKind::Other,
                format!("mount failed with status: {}", status)
            ))
        }
    }

    pub fn unmount(&mut self, lazy: bool) -> Result<()> {
        if self.mounted {
            //println!("Unmounting {}", self.dest.display());

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
                    ErrorKind::Other,
                    format!("umount failed with status: {}", status)
                ))
            }
        } else {
            Ok(())
        }
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        let _ = self.unmount(true);
    }
}
