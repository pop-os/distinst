use libc::{c_ulong, c_void, mount, swapoff as c_swapoff, umount2, MNT_DETACH, MS_BIND};
use std::ffi::CString;
use std::io::{Error, Result};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;

pub const BIND: c_ulong = MS_BIND;
// pub const SYNC: c_ulong = MS_SYNCHRONOUS;

/// Unmounts a swap partition.
pub fn swapoff<P: AsRef<Path>>(dest: P) -> Result<()> {
    unsafe {
        let swap = CString::new(dest.as_ref().as_os_str().as_bytes().to_owned());
        let swap_ptr = swap.as_ref().ok().map_or(ptr::null(), |cstr| cstr.as_ptr());

        match c_swapoff(swap_ptr) {
            0 => Ok(()),
            _err => Err(Error::last_os_error()),
        }
    }
}

/// Umounts a regular partition.
pub fn umount<P: AsRef<Path>>(dest: P, lazy: bool) -> Result<()> {
    unsafe {
        let mount = CString::new(dest.as_ref().as_os_str().as_bytes().to_owned());
        let mount_ptr = mount
            .as_ref()
            .ok()
            .map_or(ptr::null(), |cstr| cstr.as_ptr());
        match umount2(mount_ptr, if lazy { MNT_DETACH } else { 0 }) {
            0 => Ok(()),
            _err => Err(Error::last_os_error()),
        }
    }
}

/// An abstraction that will ensure that mounts are dropped in reverse.
pub struct Mounts(pub Vec<Mount>);

impl Drop for Mounts {
    fn drop(&mut self) {
        for mount in self.0.drain(..).rev() {
            drop(mount);
        }
    }
}

#[derive(Debug)]
pub struct Mount {
    source:  PathBuf,
    dest:    PathBuf,
    mounted: bool,
}

impl Mount {
    /// Mounts the specified `src` device to the `target` path, using whatever optional flags
    /// that have been specified.
    ///
    /// # Note
    ///
    /// The `fstype` should contain the file system that will be used, such as `"ext4"`,
    /// or `"vfat"`. If a file system is not valid in the context which the mount is used,
    /// then the value should be `"none"` (as in a binding).
    pub fn new<P: AsRef<Path>>(
        src: P,
        target: &Path,
        fstype: &str,
        flags: c_ulong,
        options: Option<&str>,
    ) -> Result<Mount> {
        let c_src = CString::new(src.as_ref().as_os_str().as_bytes().to_owned());
        let c_target = CString::new(target.as_os_str().as_bytes().to_owned());
        let c_fstype = CString::new(fstype.to_owned());
        let c_options = options.and_then(|options| CString::new(options.to_owned()).ok());

        let c_src = c_src
            .as_ref()
            .ok()
            .map_or(ptr::null(), |cstr| cstr.as_ptr());
        let c_target = c_target
            .as_ref()
            .ok()
            .map_or(ptr::null(), |cstr| cstr.as_ptr());
        let c_fstype = c_fstype
            .as_ref()
            .ok()
            .map_or(ptr::null(), |cstr| cstr.as_ptr());
        let c_options = c_options.as_ref().map_or(ptr::null(), |cstr| cstr.as_ptr());

        match unsafe { mount(c_src, c_target, c_fstype, flags, c_options as *const c_void) } {
            0 => Ok(Mount {
                source:  src.as_ref().to_path_buf(),
                dest:    target.to_path_buf(),
                mounted: true,
            }),
            _err => Err(Error::last_os_error()),
        }
    }

    /// Unmounts a mount, optionally unmounting with the DETACH flag.
    pub fn unmount(&mut self, lazy: bool) -> Result<()> {
        if self.mounted {
            let result = umount(self.dest(), lazy);
            if result.is_ok() {
                self.mounted = false;
            }
            result
        } else {
            Ok(())
        }
    }

    pub fn dest(&self) -> &Path { &self.dest }
}

impl Drop for Mount {
    fn drop(&mut self) { let _ = self.unmount(true); }
}
