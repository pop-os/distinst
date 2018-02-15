use disk::external::{cryptsetup_open, cryptsetup_encrypt, pvcreate};
use disk::DiskError;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct LvmEncryption {
    pub(crate) physical_volume: String,
    pub(crate) password:        Option<String>,
    pub(crate) keyfile:         Option<PathBuf>,
}

impl LvmEncryption {
    pub fn new<S: Into<Option<String>>, P: Into<Option<PathBuf>>>(
        physical_volume: String,
        password: S,
        keyfile: P,
    ) -> LvmEncryption {
        LvmEncryption {
            physical_volume,
            password: password.into(),
            keyfile: keyfile.into(),
        }
    }

    pub(crate) fn encrypt(&self, device: &Path) -> Result<(), DiskError> {
        cryptsetup_encrypt(device, self).map_err(|why| DiskError::Encryption {
            volume: device.into(),
            why,
        })
    }

    pub(crate) fn open(&self, device: &Path) -> Result<(), DiskError> {
        cryptsetup_open(device, &self.physical_volume, self).map_err(|why| {
            DiskError::EncryptionOpen {
                volume: device.into(),
                why,
            }
        })
    }

    pub(crate) fn create_physical_volume(&self) -> Result<(), DiskError> {
        let path = ["/dev/mapper/", &self.physical_volume].concat();
        pvcreate(&path).map_err(|why| DiskError::PhysicalVolumeCreate {
            volume: self.physical_volume.clone(),
            why,
        })
    }
}
