#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate derive_new;
extern crate distinst_bootloader as bootloader;
extern crate distinst_utils as misc;
extern crate distinst_external_commands as external_;
extern crate envfile;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate fstypes;
#[macro_use]
extern crate log;
extern crate partition_identity;
extern crate sys_mount;
extern crate tempdir;
extern crate libc;
extern crate rayon;
extern crate libparted;
extern crate os_detect;
extern crate disk_usage;
extern crate fstab_generate;
extern crate disk_sector;
extern crate itertools;
extern crate rand;
extern crate proc_mounts;

pub mod config;
mod error;
pub mod operations;
mod serial;
pub mod external;

pub use bootloader::{Bootloader, FORCE_BOOTLOADER};
pub use self::config::*;
pub use self::error::{DecryptionError, DiskError, PartitionError, PartitionSizeError};
pub use libparted::PartitionFlag;
use libparted::{Device, DiskType as PedDiskType};
