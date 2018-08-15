use std::io::{self, BufRead};
use std::process::Command;
use disk::{Bootloader, FileSystemSupport};

pub fn check_language_support(locale: &str) -> io::Result<Option<Vec<u8>>> {
    // Attempt to run the check-language-support external command.
    let check_language_support = Command::new("check-language-support")
        .args(&["-l", locale, "--show-installed"])
        .output();

    // If the command executed, get the standard output.
    let output = match check_language_support {
        Ok(output) => Some(output.stdout),
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(why) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("failed to spawn check-language-support: {}", why)
            ));
        }
    };

    Ok(output)
}

pub fn dependencies_of<'a, P: 'a + AsRef<str>>(deps: &[P]) -> Option<Vec<String>> {
    if deps.is_empty() {
        return None;
    }

    let deps = DependencyIterator::new(deps)
        // Recursively include dependencies of all found dependencies.
        .filter_map(|deps| dependencies_of(&deps))
        .flat_map(|deps| deps)
        // Include the dependencies we searched against, too.
        .chain(deps.iter().map(|x| x.as_ref().to_owned()))
        .collect::<Vec<String>>();

    Some(deps)
}

struct DependencyIterator<'a, P: 'a> {
    dependencies: &'a [P],
    read: usize
}

impl<'a, P: AsRef<str>> DependencyIterator<'a, P> {
    pub fn new(dependencies: &'a [P]) -> Self {
        DependencyIterator { dependencies, read: 0 }
    }
}

impl<'a, P: AsRef<str>> Iterator for DependencyIterator<'a, P> {
    type Item = Vec<String>;

    fn next(&mut self) -> Option<Vec<String>> {
        let dep = self.dependencies.get(self.read)?;
        self.read += 1;

        let output = Command::new("apt-cache")
            .args(&["show", dep.as_ref()])
            .output()
            .ok()?;

        let mut dependencies = Vec::new();

        {
            let dependencies = &mut dependencies;
            for line in io::Cursor::new(output.stdout).lines() {
                if let Ok(line) = line {
                    if ! line.starts_with("Depends:") {
                        continue
                    }

                    parse_dependency_line(
                        line[8..].trim(),
                        |dep| dependencies.push(dep.to_owned())
                    );
                }
            }
        }

        Some(dependencies)
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

pub fn get_required_packages(flags: FileSystemSupport) -> Vec<&'static str> {
    let mut retain = Vec::new();

    if flags.contains(FileSystemSupport::BTRFS) {
        retain.extend_from_slice(&["btrfs-progs", "btrfs-tools"]);
    }

    if flags.contains(FileSystemSupport::EXT4) {
        retain.push("e2fsprogs");
    }

    if flags.contains(FileSystemSupport::F2FS) {
        retain.push("f2fs-tools");
    }

    if flags.contains(FileSystemSupport::FAT) {
        retain.extend_from_slice(&["dosfstools", "fatresize"]);
    }

    if flags.contains(FileSystemSupport::NTFS) {
        retain.push("ntfs-3g");
    }

    if flags.contains(FileSystemSupport::XFS) {
        retain.push("xfsprogs");
    }

    if flags.contains(FileSystemSupport::LUKS) {
        retain.extend_from_slice(&["cryptsetup", "cryptsetup-bin"]);
    }

    if flags.contains(FileSystemSupport::LVM) {
        retain.push("lvm2");
    }

    if flags.intersects(FileSystemSupport::LVM | FileSystemSupport::LUKS) {
        retain.extend_from_slice(&["dmeventd", "dmraid"]);
    }

    match Bootloader::detect() {
        Bootloader::Efi => {
            retain.extend_from_slice(
                &["grub-efi", "grub-efi-amd64", "grub-efi-amd64-signed",
                  "shim-signed", "mokutil", "fwupdate-signed", "linux-signed-generic"]
            );
        }
        Bootloader::Bios => {
            retain.extend_from_slice(&["grub", "grub-pc"]);
        }
    }

    retain
}
