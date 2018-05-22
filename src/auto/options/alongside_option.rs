use ::OS;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub struct AlongsideOption {
    pub alongside: OS,
    pub device: PathBuf,
    pub partition: i32,
    pub sectors_free: u64,
}

impl fmt::Display for AlongsideOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Install alongside {:?} ({}): {} MiB free",
            match self.alongside {
                OS::Linux { ref info, .. } => info.pretty_name.as_str(),
                OS::Windows(ref name) => name.as_str(),
                OS::MacOs(ref name) => name.as_str()
            },
            self.device.display(),
            self.sectors_free / 2048
        )
    }
}
