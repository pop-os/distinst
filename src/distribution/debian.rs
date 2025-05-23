use crate::{
    bootloader::Bootloader,
    chroot::Chroot,
    installer::{bitflags::FileSystemSupport, traits::InstallerDiskOps},
};
use os_release::OsRelease;
use std::{
    collections::HashSet,
    env,
    io::{self, BufRead},
    process::Command,
};

pub fn check_language_support(lang: &str, chroot: &Chroot) -> io::Result<Option<String>> {
    // Takes the locale, such as `en_US.UTF-8`, and changes it into `en`.
    let locale = match lang.find('_') {
        Some(pos) => &lang[..pos],
        None => match lang.find('.') {
            Some(pos) => &lang[..pos],
            None => &lang,
        },
    };

    // Attempt to run the check-language-support external command.
    let check_language_support = chroot
        .command("check_language_support", &["-l", locale, "--show-installed"])
        .run_with_stdout();

    // If the command executed, get the standard output.
    let output = match check_language_support {
        Ok(output) => Some(output),
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(why) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("failed to spawn check-language-support: {}", why),
            ));
        }
    };

    Ok(output)
}

// This is a hack to work around issues with Ubuntu's manifest-remove file.
// This will get the immediate dependencies of the given packages.
pub fn get_dependencies_from_list<P: AsRef<str>>(deps: &[P]) -> Option<Vec<String>> {
    if deps.is_empty() {
        return None;
    }

    let mut outer = HashSet::new();

    {
        let outer = &mut outer;
        for dep in deps {
            get_dependencies_from_package(dep, |dep| {
                let dep = dep.to_owned();
                if !outer.contains(&dep) {
                    outer.insert(dep);
                }
            });
        }

        for dep in deps {
            outer.insert(dep.as_ref().to_owned());
        }
    }

    Some(outer.into_iter().collect())
}

fn get_dependencies_from_package<A: FnMut(&str), P: AsRef<str>>(dep: P, mut action: A) {
    let output = Command::new("apt-cache").args(&["show", dep.as_ref()]).output().ok();

    if let Some(output) = output {
        for line in io::Cursor::new(output.stdout).lines() {
            if let Ok(line) = line {
                if !line.starts_with("Depends:") {
                    continue;
                }

                parse_dependency_line(line[8..].trim(), |dep| action(dep));
            }
        }
    }
}

fn parse_dependency_line<F: FnMut(&str)>(line: &str, mut func: F) {
    if line.is_empty() {
        return;
    }

    for dep in line.split(',').filter_map(|dep| dep.split_whitespace().next()) {
        func(dep);
    }
}

pub fn get_bootloader_packages(os_release: &OsRelease) -> io::Result<&'static [&'static str]> {
    match (os_release.name.as_str(), os_release.version_id.as_str(), env::consts::ARCH, Bootloader::detect()) {
        (_, _, _, Bootloader::Bios) => Ok(&["grub-common", "grub2-common", "grub-pc"]),
        ("Pop!_OS", _, _, Bootloader::Efi) => Ok(&["kernelstub"]),
        ("Ubuntu", "24.04", "aarch64", Bootloader::Efi) => Ok(&[
            "efibootmgr",
            "fwupd-signed",
            "grub-efi-arm64",
            "grub-efi-arm64-signed",
            //TODO: HWE kernel not present on Ubuntu Server for ARM ISOs,
            //but non-HWE kernel not present on Ubuntu Desktop for ARM ISOs...
            "mokutil",
            "shim-signed",
        ]),
        ("Ubuntu", "24.04", "x86_64", Bootloader::Efi) => Ok(&[
            "efibootmgr",
            "fwupd-signed",
            "grub-efi-amd64",
            "grub-efi-amd64-signed",
            "linux-image-generic-hwe-24.04",
            "mokutil",
            "shim-signed",
        ]),
        ("elementary OS", _, "x86_64", Bootloader::Efi) => {
            Ok(&["grub-efi-amd64", "grub-efi-amd64-signed", "mokutil", "shim-signed"])
        },
        ("elementary OS", _, "aarch64", Bootloader::Efi) => {
            Ok(&["grub-efi-arm64", "grub-efi-arm64-signed", "mokutil", "shim-signed"])
        },
        (release, version, arch, bootloader) => Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "unsupported OS release {release:?} version {version:?} arch {arch:?} bootloader \
                 {bootloader:?}, add to distinst/src/distribution"
            ),
        )),
    }
}

pub fn get_required_packages<D: InstallerDiskOps>(
    disks: &D,
    release: &OsRelease,
) -> Vec<&'static str> {
    let flags = disks.get_support_flags();

    let mut retain = Vec::new();

    if flags.contains(FileSystemSupport::BTRFS) {
        retain.extend_from_slice(&["btrfs-progs"]);
    }

    if flags.contains(FileSystemSupport::EXT4) {
        retain.push("e2fsprogs");
    }

    if flags.contains(FileSystemSupport::F2FS) {
        retain.push("f2fs-tools");
    }

    if flags.contains(FileSystemSupport::FAT) {
        retain.push("dosfstools");
    }

    if flags.contains(FileSystemSupport::NTFS) {
        retain.push("ntfs-3g");
    }

    if flags.contains(FileSystemSupport::XFS) {
        retain.push("xfsprogs");
    }

    if flags.contains(FileSystemSupport::LUKS) {
        retain.extend_from_slice(&["cryptsetup", "cryptsetup-bin"]);
        match (release.id.as_str(), release.version.as_str()) {
            ("ubuntu", "18.10") => {
                retain.extend_from_slice(&["cryptsetup-initramfs", "cryptsetup-run"])
            }
            _ => (),
        }
    }

    if flags.intersects(FileSystemSupport::LVM | FileSystemSupport::LUKS) {
        retain.extend_from_slice(&["lvm2", "dmeventd", "dmraid", "kpartx", "kpartx-boot"]);
    }

    retain
}
