use super::FileSystemType;
use super::super::mount::Mount;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tempdir::TempDir;

/// Mounts the partition to a temporary directory and checks for the existence of an
/// installed operating system.
pub fn detect_os(device: &Path, fs: FileSystemType) -> Option<String> {
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
                    .or(detect_windows(base))
                    .or(detect_macos(base))
            })
    })
}

fn detect_linux(base: &Path) -> Option<String> {
    File::open(base.join("etc/os-release"))
        .ok()
        .and_then(|file| parse_osrelease(BufReader::new(file)))
        .or_else(|| {
            if base.join("etc").exists() {
                Some("Unknown Linux".into())
            } else {
                None
            }
        })
}

fn parse_osrelease<R: BufRead>(file: R) -> Option<String> {
    const FIELD: &str = "PRETTY_NAME=";
    file.lines()
        .flat_map(|line| line)
        .find(|line| line.starts_with(FIELD))
        .map(|line| line[FIELD.len() + 1..line.len() - 1].into())
}

fn detect_windows(base: &Path) -> Option<String> {
    // TODO: More advanced version-specific detection is possible.
    if base.join("Windows/System32/ntoskrnl.exe").exists() {
        Some("Windows".into())
    } else {
        None
    }
}

fn detect_macos(base: &Path) -> Option<String> {
    // TODO: More advanced version-specific detection is possible.
    if base.join("System/Library/CoreServices/SystemVersion.plist")
        .exists()
    {
        Some("macOS".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const OS_RELEASE: &str = r#"NAME="Pop!_OS"
VERSION="18.04 LTS (Bionic Beaver)"
ID=ubuntu
ID_LIKE=debian
PRETTY_NAME="Pop!_OS 18.04 LTS (Bionic Beaver)"
VERSION_ID="18.04"
HOME_URL="https://system76.com/pop"
SUPPORT_URL="http://support.system76.com/"
BUG_REPORT_URL="https://github.com/pop-os/pop/issues"
PRIVACY_POLICY_URL="https://system76.com/privacy"
VERSION_CODENAME=bionic
UBUNTU_CODENAME=bionic"#;

    #[test]
    fn os_release_parsing() {
        assert_eq!(
            parse_osrelease(Cursor::new(OS_RELEASE)),
            Some("Pop!_OS 18.04 LTS (Bionic Beaver)".into())
        )
    }
}