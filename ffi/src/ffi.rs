//! Contains some convenience functions and traits for handling the C API.

use std::ptr;

/// https://stackoverflow.com/a/35888360
pub trait AsMutPtr<T> {
    fn as_mut_ptr(&self) -> *mut T;
}

impl<'a, T> AsMutPtr<T> for Option<&'a mut T> {
    fn as_mut_ptr(&self) -> *mut T {
        match *self {
            Some(ref val) => unsafe { ptr::read(val) as *mut _ },
            None => ptr::null_mut(),
        }
    }
}
