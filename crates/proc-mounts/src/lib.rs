extern crate distinst_utils as misc;
extern crate libc;
extern crate sys_mount;
#[macro_use]
extern crate lazy_static;

mod mounts;
mod swaps;

use libc::swapoff as c_swapoff;
use std::collections::hash_map::DefaultHasher;
use std::ffi::CString;
use std::hash::{Hash, Hasher};
use std::io::{self, Error, ErrorKind, Read, Result};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use sys_mount::*;

pub use self::mounts::*;
pub use self::swaps::*;

lazy_static! {
    /// Static list of mounts that is dynamically updated in the background.
    pub static ref MOUNTS: Arc<RwLock<MountList>> = {
        let mounts = Arc::new(RwLock::new(MountList::new().unwrap()));
        watch_and_set(mounts.clone(), "/proc/mounts", || MountList::new().ok());
        mounts
    };
}

lazy_static! {
    /// Static list of swap points that is dynamically updated in the background.
    pub static ref SWAPS: Arc<RwLock<SwapList>> = {
        let swaps = Arc::new(RwLock::new(SwapList::new().unwrap()));
        watch_and_set(swaps.clone(), "/proc/swaps", || SwapList::new().ok());
        swaps
    };
}


/// Unmounts a swap partition using `libc::swapoff`
pub fn swapoff<P: AsRef<Path>>(dest: P) -> Result<()> {
    unsafe {
        let swap = CString::new(dest.as_ref().as_os_str().as_bytes().to_owned());
        let swap_ptr = swap.as_ref().ok().map_or(ptr::null(), |cstr| cstr.as_ptr());

        match c_swapoff(swap_ptr) {
            0 => Ok(()),
            _err => Err(Error::new(
                ErrorKind::Other,
                format!("failed to swapoff {}: {}", dest.as_ref().display(), Error::last_os_error())
            )),
        }
    }
}

/// An abstraction that will ensure that mounts are dropped in reverse.
pub struct Mounts(pub Vec<UnmountDrop<Mount>>);

impl Mounts {
    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub fn unmount(&mut self, lazy: bool) -> Result<()> {
        let flags = if lazy { UnmountFlags::DETACH } else { UnmountFlags::empty() };
        self.0.iter_mut().rev().map(|mount| mount.unmount(flags)).collect()
    }
}

impl Drop for Mounts {
    fn drop(&mut self) {
        for mount in self.0.drain(..).rev() {
            drop(mount);
        }
    }
}

fn watch_and_set<T: 'static + Send + Sync>(
    swaps: Arc<RwLock<T>>,
    file: &'static str,
    create_new: fn() -> Option<T>
) {
    thread::spawn(move || {
        let buffer: &mut [u8] = &mut [0u8; 8 * 1024];
        let modified = &mut get_file_hash(file, buffer).expect("hash could not be obtained");

        loop {
            thread::sleep(Duration::from_secs(1));
            modify_if_changed(&swaps, modified, buffer, file, create_new);
        }
    });
}

fn modify_if_changed<T: 'static + Send + Sync>(
    swaps: &Arc<RwLock<T>>,
    modified: &mut u64,
    buffer: &mut [u8],
    file: &'static str,
    create_new: fn() -> Option<T>
) {
    if let Ok(new_modified) = get_file_hash(file, buffer) {
        if new_modified != *modified {
            *modified = new_modified;
            if let Ok(ref mut swaps) = swaps.write() {
                if let Some(new_swaps) = create_new() {
                    **swaps = new_swaps;
                }
            }
        }
    }
}

fn get_file_hash<P: AsRef<Path>>(path: P, buffer: &mut [u8]) -> io::Result<u64> {
    misc::open(path).and_then(|mut file| {
        let hasher = &mut DefaultHasher::new();
        while let Ok(read) = file.read(buffer) {
            if read == 0 {
                break;
            }
            buffer[..read].hash(hasher);
        }
        Ok(hasher.finish())
    })
}
