use ::OS;
use std::path::PathBuf;

#[derive(Debug)]
pub struct AlongsideOption {
    pub alongside: OS,
    pub device: PathBuf,
    pub partition: i32,
    pub sectors_free: u64,
}
