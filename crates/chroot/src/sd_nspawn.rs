use std::{
    ffi::OsStr,
    io::Result,
    path::{Path, PathBuf},
    process::Stdio,
};
use crate::command::Command;

/// Defines the location where a `chroot` will be performed, with `systemd-nspawn`.
pub struct SystemdNspawn<'a> {
    pub path:   PathBuf,
    clear_envs: bool,
    envs:       Vec<(&'a str, &'a str)>,
}

impl<'a> SystemdNspawn<'a> {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().canonicalize()?;
        Ok(SystemdNspawn { path, clear_envs: false, envs: Vec::new() })
    }

    /// Set an environment variable to define for this chroot.
    pub fn env(&mut self, key: &'a str, value: &'a str) { self.envs.push((key, value)); }

    /// Executes an external command with `systemd-nspawn`
    pub fn command<S: AsRef<OsStr>, T: AsRef<OsStr>, I: IntoIterator<Item = T>>(
        &self,
        cmd: S,
        args: I,
    ) -> Command {
        let mut command = cascade! {
            Command::new("systemd-nspawn");
            ..args(&[
                "--bind", "/dev",
                "--bind", "/sys",
                "--bind", "/proc",
                "--bind", "/dev/mapper/control",
                "--property=DeviceAllow=block-sd rw",
                "--property=DeviceAllow=block-devices-mapper rw",
            ]);
            ..arg("-D");
            ..arg(&self.path);
            ..arg(cmd.as_ref());
            ..args(args);
            ..stderr(Stdio::piped());
            ..stdout(Stdio::piped());
        };

        for &(key, value) in &self.envs {
            command.arg(&["--setenv=", key, "=", value].concat());
        }

        command
    }
}
