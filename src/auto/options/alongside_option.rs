use os_detect::OS;
use std::{fmt, path::PathBuf};

#[derive(Debug)]
pub enum AlongsideMethod {
    Shrink { partition: i32, sectors_total: u64, sectors_free: u64, path: PathBuf },
    Free(Region),
}

#[derive(Debug)]
pub struct AlongsideOption {
    pub alongside: Option<OS>,
    pub device:    PathBuf,
    pub method:    AlongsideMethod,
}

impl AlongsideOption {
    pub fn get_os(&self) -> &str {
        match self.alongside {
            Some(OS::Linux { ref info, .. }) => info.pretty_name.as_str(),
            Some(OS::Windows(ref name)) => name.as_str(),
            Some(OS::MacOs(ref name)) => name.as_str(),
            None => "none",
        }
    }
}

impl fmt::Display for AlongsideOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let os = self.get_os();
        let device = self.device.display();

        match self.method {
            AlongsideMethod::Shrink { sectors_total, sectors_free, ref path, .. } => write!(
                f,
                "alongside {:?} ({}) by shrinking {}: {} of {} MiB free",
                os,
                device,
                path.display(),
                sectors_free / 2048,
                sectors_total / 2048
            ),
            AlongsideMethod::Free(ref region) => write!(
                f,
                "alongside {:?} ({}) using free space: {} MiB free",
                os,
                device,
                region.size() / 2048,
            ),
        }
    }
}

pub struct AlongsideData {
    pub systems:           Vec<OS>,
    pub largest_partition: i32,
    pub largest_path:      PathBuf,
    pub sectors_free:      u64,
    pub sectors_total:     u64,
    pub best_free_region:  Region,
}

#[derive(Debug)]
pub struct Region {
    pub start: u64,
    pub end:   u64,
}

impl Region {
    pub fn new(start: u64, end: u64) -> Region { Region { start, end } }

    pub fn compare(&mut self, start: u64, end: u64) {
        if self.size() < end - start {
            self.start = start;
            self.end = end;
        }
    }

    pub fn size(&self) -> u64 { self.end - self.start }
}
