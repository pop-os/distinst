extern crate bitflags;
extern crate derive_new;
pub extern crate disk_types;
extern crate distinst_bootloader as bootloader;
extern crate distinst_external_commands as external_;
extern crate distinst_utils as misc;
extern crate envfile;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate fstab_generate;
extern crate itertools;
extern crate libc;
extern crate libparted;
#[macro_use]
extern crate log;
pub extern crate distinst_disk_ops as operations;
extern crate os_detect;
extern crate partition_identity;
extern crate proc_mounts;
extern crate rand;
extern crate rayon;
extern crate sys_mount;
extern crate sysfs_class;
extern crate tempdir;

mod config;
mod error;
pub mod external;
mod serial;

pub use self::{
    config::*,
    error::{DecryptionError, DiskError, PartitionError, PartitionSizeError},
};
pub use bootloader::{Bootloader, FORCE_BOOTLOADER};
pub use libparted::PartitionFlag;
