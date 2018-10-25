#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate derive_new;
extern crate disk_sector;
extern crate disk_usage;
extern crate distinst_bootloader as bootloader;
extern crate distinst_utils as misc;
extern crate distinst_external_commands as external_;
extern crate envfile;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate fstypes;
extern crate fstab_generate;
extern crate itertools;
extern crate libc;
extern crate libparted;
#[macro_use]
extern crate log;
extern crate os_detect;
extern crate partition_identity;
extern crate proc_mounts;
extern crate rand;
extern crate rayon;
extern crate sys_mount;
extern crate sysfs_class;
extern crate tempdir;

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
