use libc;
use std::io;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use super::{Installer, Status, Error, Step};
use KILL_SWITCH;

pub struct InstallerState<'a> {
    pub installer: &'a mut Installer,
    pub status: Status,
}

impl<'a> InstallerState<'a> {
    pub fn new(installer: &'a mut Installer) -> Self {
        Self { installer, status: Status { step: Step::Init, percent: 0 }}
    }

    pub fn apply<T, F>(&mut self, step: Step, msg: &str, mut action: F) -> io::Result<T>
        where F: for<'b> FnMut(&'b mut Self) -> io::Result<T>
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
                let error = Error {
                    step: self.status.step,
                    err,
                };
                self.emit_error(&error);
                Err(error.err)
            }
        }
    }

    /// Request the caller to ask the user whether they want to keep the backup folder or not.
    pub fn emit_keep_backup_request(&mut self) -> bool {
        self.installer.emit_keep_backup_request()
    }

    /// Polls for the backup response until it is set.
    pub fn get_keep_backup_response(&mut self) -> bool {
        info!("waiting for keep backup response");
        while self.installer.backup_response.is_none() {
            thread::sleep(Duration::from_millis(16));
        }

        info!("received response");
        self.installer.backup_response.take().unwrap()
    }

    pub fn emit_status(&mut self, status: Status) {
        self.installer.emit_status(status);
    }

    pub fn emit_error(&mut self, error: &Error) {
        self.installer.emit_error(&error);
    }
}
