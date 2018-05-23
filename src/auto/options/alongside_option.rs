use super::Region;
use ::OS;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum AlongsideMethod {
    Shrink {
        partition: i32,
        sectors_free: u64,
    },
    Free(Region)
}

#[derive(Debug)]
pub struct AlongsideOption {
    pub alongside: OS,
    pub device: PathBuf,
    pub method: AlongsideMethod
}

impl AlongsideOption {
    pub fn get_os(&self) -> &str {
        match self.alongside {
            OS::Linux { ref info, .. } => info.pretty_name.as_str(),
            OS::Windows(ref name) => name.as_str(),
            OS::MacOs(ref name) => name.as_str()
        }
    }
}

impl fmt::Display for AlongsideOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let os = self.get_os();
        let device = self.device.display();

        match self.method {
            AlongsideMethod::Shrink { partition, sectors_free } => {
                write!(
                    f,
                    "Install alongside {:?} ({}) by shrinking partition {}: {} MiB free",
                    os,
                    device,
                    partition,
                    sectors_free / 2048
                )
            },
            AlongsideMethod::Free(ref region) => {
                write!(
                    f,
                    "Install alongside {:?} ({}) using free space: {} MiB free",
                    os,
                    device,
                    region.size() / 2048
                )
            }
        }
    }
}
