pub use self::disk::Disk;
pub use self::part::Partition;

mod disk;
mod part;

#[cfg(target_os = "linux")]
#[path = "linux/mod.rs"]
mod sys;
