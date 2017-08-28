use std::io::{Error, ErrorKind, Result};

use super::Partition;
use super::sys::Device;

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Disk(Device);

impl Disk {
    /// Get all disks on the system
    pub fn all() -> Result<Vec<Self>> {
        let mut disks = vec![];

        for dev in Device::all()? {
            if dev.is_disk() {
                disks.push(Disk(dev));
            }
        }

        Ok(disks)
    }

    /// Create disk from device name
    pub fn from_dev(dev: Device) -> Result<Self> {
        if dev.is_disk() {
            Ok(Disk(dev))
        } else {
            Err(Error::new(ErrorKind::NotFound, format!("{} is not a disk", dev.name())))
        }
    }

    /// Create disk from name
    pub fn from_name(name: &str) -> Result<Self> {
        Disk::from_dev(Device::new(name)?)
    }

    /// Get disk name
    pub fn name(&self) -> &str {
        self.0.name()
    }

    /// Get disk size, in bytes
    pub fn size(&self) -> Result<u64> {
        self.0.size()
    }

    /// Get disk partitions
    pub fn parts(&self) -> Result<Vec<Partition>> {
        let mut parts = vec![];

        let children = self.0.children()?;
        for dev in children {
            parts.push(Partition::from_dev(dev)?);
        }

        parts.sort();

        Ok(parts)
    }
}
