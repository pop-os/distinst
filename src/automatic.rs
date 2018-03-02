use super::{DiskExt, Disks};
use std::io;
use std::path::PathBuf;

pub struct InstallOptions {
    pub options:           Vec<InstallOption>,
    pub largest_available: u64,
    pub largest_option:    usize,
    pub needs_efi:         bool,
}

impl InstallOptions {
    /// Obtains a list of possible installation options
    pub fn detect(required_space: u64) -> io::Result<InstallOptions> {
        use FileSystemType::*;
        Disks::probe_devices()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{}", err)))
            .map(|disks| {
                let needs_efi = false;

                let options = disks
                    .get_physical_devices()
                    .iter()
                    .flat_map(|disk| {
                        let sector_size = disk.get_sector_size();

                        disk.get_partitions().iter().filter_map(move |partition| {
                            match partition.filesystem {
                                // Ignore partitions that are used as swap
                                Some(Swap) => None,
                                // If the partition has a file system, determine if it is
                                // shrinkable, and whether or not it has an OS installed
                                // on it.
                                Some(_) => match partition.sectors_used(sector_size) {
                                    Some(Ok(used)) => match partition.probe_os() {
                                        // Ensure the other OS has at least 5GB of headroom
                                        Some(os) => {
                                            // The size that the OS partition could be shrunk to
                                            let shrunk_os = used + (5242880 / sector_size);
                                            // The maximum size that we can allocate to the new
                                            // install
                                            let install_size = partition.sectors() - shrunk_os;
                                            // Whether there is enough room or not in the install.
                                            if install_size > required_space {
                                                Some(InstallOption::Os(os, shrunk_os, install_size))
                                            } else {
                                                None
                                            }
                                        }
                                        // If it's just a data partition, ensure it has at least
                                        // 1GB of headroom
                                        None => {
                                            let shrink_to = used + (1048576 / sector_size);
                                            let install_size = partition.sectors() - shrink_to;
                                            if install_size > required_space {
                                                Some(InstallOption::Partition(
                                                    partition.get_device_path().into(),
                                                    shrink_to,
                                                    install_size,
                                                ))
                                            } else {
                                                None
                                            }
                                        }
                                    },
                                    Some(Err(why)) => {
                                        error!(
                                            "unable to get usage for {}: {}. skipping partition",
                                            partition.device_path.display(),
                                            why
                                        );
                                        None
                                    }
                                    // The partition doesn't support shrinking, so we will skip it.
                                    None => None,
                                },
                                // If the partition does not have a file system
                                None => {
                                    if partition.sectors() > required_space {
                                        Some(InstallOption::Overwrite(
                                            partition.get_device_path().into(),
                                            partition.sectors(),
                                        ))
                                    } else {
                                        None
                                    }
                                }
                            }
                        })
                    })
                    .collect::<Vec<InstallOption>>();

                // TODO: Calculate
                let (largest, available) = (0, 0);

                InstallOptions {
                    options,
                    largest_available: available,
                    largest_option: largest,
                    needs_efi,
                }
            })
    }
}

pub enum InstallOption {
    Os(String, u64, u64),
    Partition(PathBuf, u64, u64),
    Overwrite(PathBuf, u64),
}
