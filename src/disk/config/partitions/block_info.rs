use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

/// Information that will be used to generate a fstab entry for the given partition.
pub(crate) struct BlockInfo {
    pub uuid:    OsString,
    pub mount:   Option<PathBuf>,
    pub fs:      &'static str,
    pub options: String,
    pub dump:    bool,
    pub pass:    bool,
}

impl BlockInfo {
    pub fn mount(&self) -> &OsStr {
        self.mount
            .as_ref()
            .map_or(OsStr::new("none"), |path| path.as_os_str())
    }

    /// The size of the data contained within.
    pub fn len(&self) -> usize {
        self.uuid.len() + self.mount().len() + self.fs.len() + self.options.len() + 2
    }
}
