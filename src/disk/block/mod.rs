pub use self::imp::BlockDev;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;
