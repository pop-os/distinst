use super::FileSystemType;
use super::super::mount::Mount;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tempdir::TempDir;

pub fn detect_os(device: &Path, fs: FileSystemType) -> Option<String> {
    // Mount a temporary directoy where we will mount the FS.
    TempDir::new("distinst").ok().and_then(|tempdir| {
        // Mount the FS to the temporary directory
        Mount::new(
            device,
            &tempdir.path(),
            match fs {
                FileSystemType::Fat16 | FileSystemType::Fat32 => "vfat",
                fs => fs.into(),
            },
            0,
            None,
        ).ok()
            .and_then(|_mount| {
                // Check if the partition contains `/etc/lsb-release` to detect a Linux
                // install.
                let base = tempdir.path();
                detect_linux(base)
                    .or(detect_windows(base))
                    .or(detect_macos(base))
            })
    })
}

fn detect_linux(base: &Path) -> Option<String> {
    const FIELD: &str = "PRETTY_NAME=";
    File::open(base.join("etc/os-release"))
        .ok()
        .and_then(|file| {
            for line in BufReader::new(file).lines() {
                if let Ok(line) = line {
                    if line.starts_with(FIELD) {
                        return Some(line[FIELD.len()..line.len() - 1].into());
                    }
                }
            }
            None
        })
        .or_else(|| {
            if base.join("etc").exists() {
                Some("Unknown Linux".into())
            } else {
                None
            }
        })
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
