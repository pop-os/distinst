pub use self::imp::Device;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;
