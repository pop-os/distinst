use std::{fmt, path::PathBuf};

pub const IS_ROTATIONAL: u8 = 1;
pub const IS_REMOVABLE: u8 = 2;
pub const MEETS_REQUIREMENTS: u8 = 4;

#[derive(Debug)]
pub struct EraseOption {
    pub device:  PathBuf,
    pub model:   String,
    pub sectors: u64,
    pub flags:   u8,
}

impl fmt::Display for EraseOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Erase and Install to {} ({})", self.model, self.device.display())
    }
}

impl EraseOption {
    pub fn is_rotational(&self) -> bool { self.flags & IS_ROTATIONAL != 0 }

    pub fn is_removable(&self) -> bool { self.flags & IS_REMOVABLE != 0 }

    pub fn meets_requirements(&self) -> bool { self.flags & MEETS_REQUIREMENTS != 0 }

    pub fn get_linux_icon(&self) -> &'static str {
        const BOTH: u8 = IS_ROTATIONAL | IS_REMOVABLE;
        match self.flags & BOTH {
            BOTH => "drive-harddisk-usb",
            IS_ROTATIONAL => "drive-harddisk-scsi",
            IS_REMOVABLE => "drive-removable-media-usb",
            0 => "drive-harddisk-solidstate",
            _ => unreachable!("get_linux_icon(): branch not handled"),
        }
    }
}
