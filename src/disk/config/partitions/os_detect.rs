use super::super::super::mount::Mount;
use super::FileSystemType;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tempdir::TempDir;
use os_release::OsRelease;
use misc::get_uuid;

/// Adds a new map method for boolean types.
pub trait BoolExt {
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

#[derive(Debug)]
#[allow(large_enum_variant)]
pub enum OS {
    Windows(String),
    Linux {
        info: OsRelease,
        efi: Option<String>,
        home: Option<String>,
        recovery: Option<String>
    },
    MacOs(String)
}

/// Mounts the partition to a temporary directory and checks for the existence of an
/// installed operating system.
///
/// If the installed operating system is Linux, it will also report back the location
/// of the home partition.
pub fn detect_os(device: &Path, fs: FileSystemType) -> Option<OS> {
    let fs = match fs {
        FileSystemType::Fat16 | FileSystemType::Fat32 => "vfat",
        fs => fs.into(),
    };

    // Create a temporary directoy where we will mount the FS.
    TempDir::new("distinst").ok().and_then(|tempdir| {
        // Mount the FS to the temporary directory
        let base = tempdir.path();
        Mount::new(device, base, fs, 0, None)
            .ok()
            .and_then(|_mount| {
                detect_linux(base)
                    .or_else(|| detect_windows(base))
                    .or_else(|| detect_macos(base))
            })
    })
}

fn find_linux_parts(base: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let parse_fstab_mount = move |mount: &str| -> Option<String> {
        if mount.starts_with('/') {
            get_uuid(Path::new(mount))
        } else if mount.starts_with("UUID") {
            let (_, uuid) = mount.split_at(5);
            Some(uuid.into())
        } else {
            error!("unsupported mount type: {}", mount);
            None
        }
    };

    let mut home = None;
    let mut efi = None;
    let mut recovery = None;

    if let Ok(fstab) = File::open(base.join("etc/fstab")) {
        for entry in BufReader::new(fstab).lines() {
            if let Ok(entry) = entry {
                let entry = entry.trim();

                if entry.starts_with('#') {
                    continue;
                }

                let mut fields = entry.split_whitespace();
                let source = fields.next();
                let target = fields.next();

                if let Some(target) = target {
                    if home.is_none() && target == "/home" {
                        if let Some(path) = parse_fstab_mount(source.unwrap()) {
                            home = Some(path);
                        }
                    } else if efi.is_none() && target == "/boot/efi" {
                        if let Some(path) = parse_fstab_mount(source.unwrap()) {
                            efi = Some(path);
                        }
                    } else if recovery.is_none() && target == "/recovery" {
                        if let Some(path) = parse_fstab_mount(source.unwrap()) {
                            recovery = Some(path);
                        }
                    }
                }
            }
        }
    }

    (home, efi, recovery)
}

fn detect_linux(base: &Path) -> Option<OS> {
    let path = base.join("etc/os-release");
    if path.exists() {
        if let Ok(os_release) = OsRelease::new_from(path) {
            let (home, efi, recovery) = find_linux_parts(base);
            return Some(OS::Linux {
                info: os_release,
                home,
                efi,
                recovery,
            });
        }
    }

    None
}

fn detect_windows(base: &Path) -> Option<OS> {
    // TODO: More advanced version-specific detection is possible.
    base.join("Windows/System32/ntoskrnl.exe")
        .exists()
        .map(|| OS::Windows("Windows".into()))
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
                "<key>ProductUserVisibleVersion</key>" => flags |= 1,
                "<key>ProductName</key>" => flags |= 2,
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

fn detect_macos(base: &Path) -> Option<OS> {
    File::open(base.join("etc/os-release"))
        .ok()
        .and_then(|file| {
            parse_plist(BufReader::new(file))
                .or_else(|| Some("Mac OS (Unknown)".into()))
                .map(|name| OS::MacOs(name))
        })
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
