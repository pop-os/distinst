use std::io::{Error, ErrorKind, Result};
use std::path::PathBuf;

use super::sys::{Device, Mount, Swap};

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Partition(Device);

impl Partition {
    /// Get all partitions on the system
    pub fn all() -> Result<Vec<Self>> {
        let mut parts = vec![];

        for dev in Device::all()? {
            if dev.is_part() {
                parts.push(Partition(dev));
            }
        }

        Ok(parts)
    }

    /// Create partition from device
    pub fn from_dev(dev: Device) -> Result<Self> {
        if dev.is_part() {
            Ok(Partition(dev))
        } else {
            Err(Error::new(ErrorKind::NotFound, format!("{} is not a partition", dev.name())))
        }
    }

    /// Create partition from name
    pub fn from_name(name: &str) -> Result<Self> {
        Partition::from_dev(Device::new(name)?)
    }

    /// Get partition name
    pub fn name(&self) -> &str {
        self.0.name()
    }

    /// Get partition path
    pub fn path(&self) -> PathBuf {
        self.0.path()
    }

    /// Get partition size, in bytes
    pub fn size(&self) -> Result<u64> {
        self.0.size()
    }

    /// Get the current mount point of the device
    pub fn mounts(&self) -> Result<Vec<Mount>> {
        self.0.mounts()
    }

    /// Get the current swap point of the device
    pub fn swaps(&self) -> Result<Vec<Swap>> {
        self.0.swaps()
    }
}
