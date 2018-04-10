use disk::external::{cryptsetup_encrypt, cryptsetup_open, pvcreate};
use disk::DiskError;
use std::path::{Path, PathBuf};

/// A structure which contains the encryption settings for a physical volume.
#[derive(Debug, Clone, PartialEq)]
pub struct LvmEncryption {
    pub(crate) physical_volume: String,
    pub(crate) password:        Option<String>,
    pub(crate) keydata:         Option<(String, Option<(PathBuf, PathBuf)>)>,
}

impl LvmEncryption {
    pub fn new<S: Into<Option<String>>>(
        physical_volume: String,
        password: S,
        keydata: S,
    ) -> LvmEncryption {
        LvmEncryption {
            physical_volume,
            password: password.into(),
            keydata: keydata.into().map(|key| (key, None)),
        }
    }

    /// Encrypts a new partition with the settings stored in the structure.
    pub(crate) fn encrypt(&self, device: &Path) -> Result<(), DiskError> {
        cryptsetup_encrypt(device, self).map_err(|why| DiskError::Encryption {
            volume: device.into(),
            why,
        })
    }

    /// Opens the previously-encrypted partition with the same settings used to
    /// encrypt it.
    pub(crate) fn open(&self, device: &Path) -> Result<(), DiskError> {
        cryptsetup_open(device, self).map_err(|why| DiskError::EncryptionOpen {
            volume: device.into(),
            why,
        })
    }

    /// Creates a physical volume
    pub(crate) fn create_physical_volume(&self) -> Result<(), DiskError> {
        let path = ["/dev/mapper/", &self.physical_volume].concat();
        pvcreate(&path).map_err(|why| DiskError::PhysicalVolumeCreate {
            volume: self.physical_volume.clone(),
            why,
        })
    }
}
