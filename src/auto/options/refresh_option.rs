use os_release::OsRelease;
use partition_identity::PartitionID;
use std::fmt;

#[derive(Debug)]
pub struct RefreshOption {
    pub os_release:     OsRelease,
    pub root_part:      String,
    pub home_part:      Option<PartitionID>,
    pub efi_part:       Option<PartitionID>,
    pub recovery_part:  Option<PartitionID>,
    pub can_retain_old: bool,
}

impl fmt::Display for RefreshOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let root_part: String =
            match PartitionID::new_uuid(self.root_part.clone()).get_device_path() {
                Some(uuid) => uuid.to_string_lossy().into(),
                None => "None".into(),
            };

        write!(f, "Refresh {} on {}", self.os_release.name, root_part)
    }
}
