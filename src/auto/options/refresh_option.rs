use os_release::OsRelease;
use std::fmt;

#[derive(Debug)]
pub struct RefreshOption {
    pub os_release:     OsRelease,
    pub root_part:      String,
    pub root:           os_detect::LinuxPartition,
    pub home_part:      Option<os_detect::LinuxPartition>,
    pub efi_part:       Option<os_detect::LinuxPartition>,
    pub recovery_part:  Option<os_detect::LinuxPartition>,
    pub can_retain_old: bool,
}

impl fmt::Display for RefreshOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let root_part: String =
            match self.root.source.get_device_path() {
                Some(path) => path.to_string_lossy().into(),
                None => "None".into(),
            };

        write!(f, "Refresh {} on {}", self.os_release.name, root_part)
    }
}
