use super::{Error, Installer, Status, Step};
use libc;
use std::{io, sync::atomic::Ordering};
use crate::KILL_SWITCH;

pub struct InstallerState<'a> {
    pub installer: &'a mut Installer,
    pub status:    Status,
}

impl<'a> InstallerState<'a> {
    pub fn new(installer: &'a mut Installer) -> Self {
        Self { installer, status: Status { step: Step::Init, percent: 0 } }
    }

    pub fn apply<T, F>(&mut self, step: Step, msg: &str, mut action: F) -> io::Result<T>
    where
        F: for<'c> FnMut(&'c mut Self) -> io::Result<T>,
    {
        unsafe {
            libc::sync();
        }

        if KILL_SWITCH.load(Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "process killed"));
        }

        self.status.step = step;
        self.status.percent = 0;
        let status = self.status;
        self.emit_status(status);

        info!("starting {} step", msg);
        match action(self) {
            Ok(value) => Ok(value),
            Err(err) => {
                error!("{} error: {}", msg, err);
                let error = Error { step: self.status.step, err };
                self.emit_error(&error);
                Err(error.err)
            }
        }
    }

    pub fn emit_status(&mut self, status: Status) { self.installer.emit_status(status); }

    pub fn emit_error(&mut self, error: &Error) { self.installer.emit_error(&error); }
}
