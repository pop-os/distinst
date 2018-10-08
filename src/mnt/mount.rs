use libc::swapoff as c_swapoff;
use std::ffi::CString;
use std::io::{Error, ErrorKind, Result};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use sys_mount::*;

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
