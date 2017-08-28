use std::io::{Error, ErrorKind, Result};

use super::BlockDev;

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Partition(BlockDev);

impl Partition {
    /// Get all partitions on the system
    pub fn all() -> Result<Vec<Self>> {
        let mut parts = vec![];

        for dev in BlockDev::all()? {
            if dev.is_part() {
                parts.push(Partition(dev));
            }
        }

        Ok(parts)
    }

    /// Create partition from device
    pub fn from_dev(dev: BlockDev) -> Result<Self> {
        if dev.is_part() {
            Ok(Partition(dev))
        } else {
            Err(Error::new(ErrorKind::NotFound, format!("{} is not a partition", dev.name())))
        }
    }

    /// Create partition from name
    pub fn from_name(name: &str) -> Result<Self> {
        Partition::from_dev(BlockDev::new(name)?)
    }

    /// Get partition name
    pub fn name(&self) -> &str {
        self.0.name()
    }

    /// Get partition size, in bytes
    pub fn size(&self) -> Result<u64> {
        self.0.size()
    }
}
