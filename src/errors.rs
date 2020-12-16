use crate::disks::DiskError;
use std::{error::Error, fmt::Display, io};

/// Extends `Option<T>` to be converted into an `io::Result<T>`.
pub trait IntoIoResult<T> {
    fn into_io_result<E, F>(self, error: F) -> io::Result<T>
    where
        E: Into<Box<dyn Error + Send + Sync>>,
        F: FnMut() -> E;
}

impl<T> IntoIoResult<T> for Option<T> {
    fn into_io_result<E, F>(self, mut error: F) -> io::Result<T>
    where
        E: Into<Box<dyn Error + Send + Sync>>,
        F: FnMut() -> E,
    {
        self.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, error()))
    }
}

/// Extends `io::Result<T>` to enable supplying additional context to an I/O error.
pub trait IoContext<T> {
    fn with_context<F: FnMut(Box<dyn Display>) -> String>(self, func: F) -> io::Result<T>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn with_context<F: FnMut(Box<dyn Display>) -> String>(self, mut func: F) -> io::Result<T> {
        self.map_err(|why| io::Error::new(why.kind(), func(Box::new(why))))
    }
}

// NOTE: This can be removed once RFC #1210 is implemented.
impl<T> IoContext<T> for Result<T, DiskError> {
    fn with_context<F: FnMut(Box<dyn Display>) -> String>(self, mut func: F) -> io::Result<T> {
        self.map_err(|why| io::Error::new(io::ErrorKind::Other, func(Box::new(why))))
    }
}

// Requires RFC #1210: https://github.com/rust-lang/rust/issues/37653
// default impl<T, E: Display> IoContext<T> for Result<T, E> {
//     fn with_context<F: FnMut(Box<Display>) -> String>(self, mut func: F) -> io::Result<T> {
//         self.map_err(|why| io::Error::new(io::ErrorKind::Other, func(Box::new(why))))
//     }
// }
