//! Provides the means for for detecting the existence of an OS from an unmounted device, or path.
//!
//! ```rust,no_run
//! extern crate os_detect;
//!
//! use os_detect::detect_os_from_device;
//! use std::path::Path;
//!
//! pub fn main() {
//!     let device_path = &Path::new("/dev/sda3");
//!     let fs = "ext4";
//!     if let Some(os) = detect_os_from_device(device_path, fs) {
//!         println!("{:#?}", os);
//!     }
//! }
//! ```

#[macro_use]
extern crate log;
extern crate os_release;
extern crate partition_identity;
extern crate sys_mount;
extern crate tempdir;

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use tempdir::TempDir;
use os_release::OsRelease;
use std::path::PathBuf;
use partition_identity::PartitionID;
use sys_mount::*;

#[derive(Debug, Clone)]
pub struct LinuxPartition {
    pub source: PartitionID,
    pub dest: PathBuf,
    pub fs: String,
    pub options: String,
}

/// Describes the OS found on a partition.
#[derive(Debug, Clone)]
pub enum OS {
    Windows(String),
    Linux {
        info: OsRelease,
        partitions: Vec<LinuxPartition>
    },
    MacOs(String)
}

/// Mounts the partition to a temporary directory and checks for the existence of an
/// installed operating system.
///
/// If the installed operating system is Linux, it will also report back the location
/// of the home partition.
pub fn detect_os_from_device<'a>(device: &Path, subvol: Option<&str>, fs: impl Into<FilesystemType<'a>>) -> Option<OS> {
    info!("detecting OS from device: {:?}", device);
    // Create a temporary directoy where we will mount the FS.
    TempDir::new("distinst").ok().and_then(|tempdir| {
        // Mount the FS to the temporary directory
        let base = tempdir.path();

        let data = if let Some(subvol) = subvol {
            ["subvol=", subvol].concat()
        } else {
            String::new()
        };

        let fs = fs.into();

        info!("mounting {:?} {:?} {}", device, fs, data);

        let mount = Mount::builder()
            .data(&*data)
            .fstype(fs)
            .mount_autodrop(device, base, UnmountFlags::DETACH);

        if let Err(why) = mount {
            error!("failed to mount device for probing: {:?}", why);
            return None
        }

        detect_os_from_path(base)
    })
}

/// Detects the existence of an OS at a defined path.
///
/// This function is called by `detect_os_from_device`, after having temporarily mounted it.
pub fn detect_os_from_path(base: &Path) -> Option<OS> {
    info!("detecting OS from {:?}", base);
    if let Ok(mut dir) = std::fs::read_dir(base) {
        while let Some(Ok(entry)) = dir.next() {
            info!("Found {:?}", entry.path());
        }
    }

    detect_linux(base)
        .or_else(|| detect_windows(base))
        .or_else(|| detect_macos(base))
}

/// Detect if Linux is installed at the given path.
pub fn detect_linux(base: &Path) -> Option<OS> {
    let path = base.join("etc/os-release");
    if path.exists() {
        info!("found OS Release: {}", std::fs::read_to_string(&path).unwrap());
        if let Ok(info) = OsRelease::new_from(path) {
            return Some(OS::Linux {
                info,
                partitions: find_linux_parts(base)
            });
        }
    }

    None
}

/// Detect if Mac OS is installed at the given path.
pub fn detect_macos(base: &Path) -> Option<OS> {
    open(base.join("etc/os-release"))
        .ok()
        .and_then(|file| {
            parse_plist(BufReader::new(file))
                .or_else(|| Some("Mac OS (Unknown)".into()))
                .map(OS::MacOs)
        })
}

/// Detect if Windows is installed at the given path.
pub fn detect_windows(base: &Path) -> Option<OS> {
    // TODO: More advanced version-specific detection is possible.
    base.join("Windows/System32/ntoskrnl.exe")
        .exists()
        .map(|| OS::Windows("Windows".into()))
}

fn find_linux_parts(base: &Path) -> Vec<LinuxPartition> {
    let mut partitions = Vec::new();

    if let Ok(fstab) = open(base.join("etc/fstab")) {
        for entry in BufReader::new(fstab).lines().flatten() {
            let entry = entry.trim();
            if entry.starts_with('#') || entry.is_empty() {
                continue;
            }

            let mut fields = entry.split_whitespace();
            let source = fields.next();
            let dest = fields.next();
            let fs = fields.next();
            let options = fields.next();

            if let Some(((dest, fs), options)) = dest.zip(fs).zip(options) {
                if let Some(Ok(source)) = source.map(|s| s.parse::<PartitionID>()) {
                    partitions.push(LinuxPartition {
                        source,
                        dest: PathBuf::from(dest.to_owned()),
                        fs: fs.to_owned(),
                        options: options.to_owned()
                    })
                }
            }
        }
    }

    partitions
}

fn parse_plist<R: BufRead>(file: R) -> Option<String> {
    // The plist is an XML file, but we don't need complex XML parsing for this.
    let mut product_name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut flags = 0;

    for entry in file.lines().flat_map(|line| line) {
        let entry = entry.trim();
        match flags {
            0 => match entry {
                "<key>ProductUserVisibleVersion</key>" => flags = 1,
                "<key>ProductName</key>" => flags = 2,
                _ => (),
            },
            1 => {
                if entry.len() < 10 {
                    return None;
                }
                version = Some(entry[8..entry.len() - 9].into());
                flags = 0;
            }
            2 => {
                if entry.len() < 10 {
                    return None;
                }
                product_name = Some(entry[8..entry.len() - 9].into());
                flags = 0;
            }
            _ => unreachable!(),
        }
        if product_name.is_some() && version.is_some() {
            break;
        }
    }

    if let (Some(name), Some(version)) = (product_name, version) {
        Some(format!("{} ({})", name, version))
    } else {
        None
    }
}

fn open<P: AsRef<Path>>(path: P) -> io::Result<File> {
    File::open(&path).map_err(|why| io::Error::new(
        io::ErrorKind::Other,
        format!("unable to open file at {:?}: {}", path.as_ref(), why)
    ))
}

/// Adds a new map method for boolean types.
pub(crate) trait BoolExt {
    fn map<T, F: Fn() -> T>(&self, action: F) -> Option<T>;
}

impl BoolExt for bool {
    fn map<T, F: Fn() -> T>(&self, action: F) -> Option<T> {
        if *self {
            Some(action())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const MAC_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "Apple Stuff">
<plist version="1.0">
<dict>
    <key>ProductBuildVersion</key>
    <string>10C540</string>
    <key>ProductName</key>
    <string>Mac OS X</string>
    <key>ProductUserVisibleVersion</key>
    <string>10.6.2</string>
    <key>ProductVersion</key>
    <string>10.6.2</string>
</dict>
</plist>"#;

    #[test]
    fn mac_plist_parsing() {
        assert_eq!(
            parse_plist(Cursor::new(MAC_PLIST)),
            Some("Mac OS X (10.6.2)".into())
        );
    }
}
