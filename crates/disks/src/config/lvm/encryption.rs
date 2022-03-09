use crate::external::{cryptsetup_encrypt, cryptsetup_open, pvcreate};
use crate::DiskError;
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// A structure which contains the encryption settings for a physical volume.
#[derive(Clone, PartialEq)]
pub struct LuksEncryption {
    pub physical_volume: String,
    pub password:        Option<String>,
    pub keydata:         Option<(String, Option<(PathBuf, PathBuf)>)>,
    pub filesystem:      disk_types::FileSystem,
}

impl fmt::Debug for LuksEncryption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "LuksEncryption {{ physical_volume: {}, password: hidden, keydata: {:?} }}",
            self.physical_volume, self.keydata
        )
    }
}

impl LuksEncryption {
    pub fn new<S: Into<Option<String>>>(
        physical_volume: String,
        password: S,
        keydata: S,
        filesystem: disk_types::FileSystem,
    ) -> LuksEncryption {
        LuksEncryption {
            physical_volume,
            password: password.into(),
            keydata: keydata.into().map(|key| (key, None)),
            filesystem,
        }
    }

    /// Encrypts a new partition with the settings stored in the structure.
    pub fn encrypt(&self, device: &Path) -> Result<(), DiskError> {
        cryptsetup_encrypt(device, self)
            .map_err(|why| DiskError::Encryption { volume: device.into(), why })
    }

    /// Opens the previously-encrypted partition with the same settings used to
    /// encrypt it.
    pub fn open(&self, device: &Path) -> Result<(), DiskError> {
        cryptsetup_open(device, self)
            .map_err(|why| DiskError::EncryptionOpen { volume: device.into(), why })
    }

    /// Creates a physical volume
    pub fn create_physical_volume(&self) -> Result<(), DiskError> {
        let path = ["/dev/mapper/", &self.physical_volume].concat();
        pvcreate(&path).map_err(|why| DiskError::PhysicalVolumeCreate {
            volume: self.physical_volume.clone(),
            why,
        })
    }
}
