use envfile::EnvFile;
use crate::errors::IoContext;
use std::io;

#[derive(AsMut, Deref, DerefMut)]
#[as_mut]
#[deref]
#[deref_mut]
pub struct RecoveryEnv(EnvFile);

impl RecoveryEnv {
    pub fn new() -> io::Result<Self> {
        let env = EnvFile::new("/cdrom/recovery.conf").with_context(|source| {
            format!("error parsing envfile at /cdrom/recovery.conf: {}", source)
        })?;

        Ok(Self(env))
    }

    pub fn remove(&mut self, key: &str) { self.0.store.remove(key); }

    pub fn write(&mut self) -> io::Result<()> {
        self.0
            .write()
            .with_context(|source| format!("failed to write changes to recovery conf: {}", source))
    }
}
