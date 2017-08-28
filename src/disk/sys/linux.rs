use std::fs;
use std::io::{Error, ErrorKind, Read, Result};

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Device(String);

impl Device {
    /// Get all block devices on the system
    pub fn all() -> Result<Vec<Self>> {
        let mut devs = vec![];

        for entry_res in fs::read_dir("/sys/class/block/")? {
            let entry = entry_res?;
            let name = entry.file_name().into_string().map_err(|os_str| {
                Error::new(ErrorKind::InvalidData, format!("Invalid device name: {:?}", os_str))
            })?;
            devs.push(Device(name));
        }

        devs.sort();

        Ok(devs)
    }

    /// Create a block device by name
    pub fn new(name: &str) -> Result<Self> {
        fs::read_dir(&format!("/sys/class/block/{}/", name))?;
        Ok(Device(name.to_string()))
    }

    /// Get the name of the block device
    pub fn name(&self) -> &str {
        &self.0
    }

    /// Check if this device is a disk
    pub fn is_disk(&self) -> bool {
        fs::read_dir(&format!("/sys/block/{}/", self.0)).is_ok()
    }

    /// Check if this device is a partition
    pub fn is_part(&self) -> bool {
        fs::File::open(&format!("/sys/class/block/{}/partition", self.0)).is_ok()
    }

    /// Get major and minor device numbers
    pub fn dev(&self) -> Result<(u32, u32)> {
        let mut file = fs::File::open(&format!("/sys/class/block/{}/dev", self.0))?;

        let mut data = String::new();
        file.read_to_string(&mut data)?;

        let mut ids = data.trim().split(':');

        let major = ids.next().ok_or(
            Error::new(ErrorKind::InvalidData, "No major device number")
        )?.parse::<u32>().map_err(|err|
            Error::new(ErrorKind::InvalidData, format!("Invalid major device number: {}", err))
        )?;

        let minor = ids.next().ok_or(
            Error::new(ErrorKind::InvalidData, "No minor device number")
        )?.parse::<u32>().map_err(|err|
            Error::new(ErrorKind::InvalidData, format!("Invalid minor device number: {}", err))
        )?;

        Ok((major, minor))
    }

    /// Get the size of the disk in bytes
    pub fn size(&self) -> Result<u64> {
        let mut file = fs::File::open(&format!("/sys/class/block/{}/size", self.0))?;

        let mut data = String::new();
        file.read_to_string(&mut data)?;

        let sectors = data.trim().parse::<u64>().map_err(|err| {
            Error::new(ErrorKind::InvalidData, format!("Invalid disk size: {}", err))
        })?;

        Ok(sectors * 512)
    }

    /// Get the children of the device
    pub fn children(&self) -> Result<Vec<Device>> {
        let mut devs = vec![];

        if self.is_disk() {
            for entry_res in fs::read_dir(&format!("/sys/block/{}/", self.0))? {
                let entry = entry_res?;
                if let Ok(name) = entry.file_name().into_string() {
                    if name.starts_with(&self.0) {
                        if let Ok(dev) = Device::new(&name) {
                            if dev.is_part() {
                                devs.push(dev);
                            }
                        }
                    }
                }
            }
        }

        devs.sort();

        Ok(devs)
    }
}
