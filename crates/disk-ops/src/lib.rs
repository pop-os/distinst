//! A collection of tools for modifying partitions and disk layouts.

#[macro_use]
extern crate derive_new;
extern crate disk_types;
extern crate distinst_bootloader as bootloader;
extern crate distinst_external_commands as external;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate rayon;
#[macro_use]
extern crate smart_default;
extern crate sys_mount;
extern crate tempdir;

mod mklabel;
mod mkpart;
mod mvpart;
mod ops;
pub mod parted;
mod resize;
mod rmpart;

pub use self::{mklabel::*, mkpart::*, mvpart::*, ops::*, resize::*, rmpart::*};

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
