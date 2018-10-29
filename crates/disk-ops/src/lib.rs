#[macro_use]
extern crate derive_new;
extern crate disk_types;
extern crate distinst_external_commands as external;
extern crate distinst_bootloader as bootloader;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate rayon;
extern crate sys_mount;
extern crate tempdir;

mod mklabel;
mod mkpart;
mod mvpart;
mod ops;
mod resize;
mod rmpart;
pub mod parted;

pub use self::mklabel::*;
pub use self::mkpart::*;
pub use self::mvpart::*;
pub use self::ops::*;
pub use self::resize::*;
pub use self::rmpart::*;

const MEBIBYTE: u64 = 1_048_576;
const MEGABYTE: u64 = 1_000_000;

/// Defines the start and end sectors of a partition on the disk.
#[derive(new)]
pub struct BlockCoordinates {
    pub start: u64,
    pub end:   u64,
}

impl BlockCoordinates {
    /// Modifies the coordinates based on the new length that is supplied.BlockCoordinates
    /// will be adjusted automatically based on whether the partition is shrinking or
    /// growing.
    pub fn resize_to(&mut self, new_len: u64) {
        let offset = (self.end - self.start) as i64 - new_len as i64;
        if offset < 0 {
            self.end += offset.abs() as u64;
        } else {
            self.end -= offset as u64;
        }
    }
}

/// Defines how many sectors to skip, and how the partition is.
#[derive(Clone, Copy)]
pub struct OffsetCoordinates {
    pub skip:   u64,
    pub offset: i64,
    pub length: u64,
}
