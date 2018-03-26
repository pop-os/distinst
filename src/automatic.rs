use super::{Bootloader, DiskExt, Disks};
use libparted::PartitionFlag;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub struct InstallOptions {
    /// A list of possible installation options.
    pub options: Vec<InstallOption>,
    /// The size of the largest installation option.
    pub largest_available: u64,
    /// The ID of the largest installation option in `options`.
    pub largest_option: usize,
    /// A list of ESP partitions discovered in the system.
    pub efi_partitions: Option<Vec<PathBuf>>,
}

impl InstallOptions {
    /// Obtains a list of possible installation options
    pub fn detect(disks: &Disks, required_space: u64) -> io::Result<InstallOptions> {
        use FileSystemType::*;

        let efi_partitions = if Bootloader::detect() == Bootloader::Efi {
            let partitions = disks
                .get_physical_devices()
                .iter()
                .flat_map(|d| d.get_partitions().iter());

            Some(
                partitions
                    .filter(|partition| {
                        (partition.filesystem == Some(Fat16) || partition.filesystem == Some(Fat32))
                            && partition.flags.contains(&PartitionFlag::PED_PARTITION_ESP)
                    })
                    .map(|partition| partition.get_device_path().to_path_buf())
                    .collect::<Vec<PathBuf>>(),
            )
        } else {
            None
        };

        let devices = disks
            // Obtain physical partitions and sector sizes.
            .get_physical_devices()
            .iter()
            .flat_map(|disk| {
                let ss: u64 = disk.get_sector_size();
                disk.get_partitions().iter().map(move |p| (true, ss, p))
            })
            // Then obtain logical partitions and their sector sizes.
            .chain(disks.get_logical_devices().iter().flat_map(|disk| {
                let ss = disk.get_sector_size();
                disk.get_partitions().iter().map(move |p| (false, ss, p))
            }));

        let options = devices
            .filter_map(move |(is_physical, sector_size, partition)| {
                match partition.filesystem {
                    // Ignore partitions that are used as swap
                    Some(Swap) => None,
                    // If the partition has a file system, determine if it is
                    // shrinkable, and whether or not it has an OS installed
                    // on it.
                    Some(_) if is_physical => match partition.sectors_used(sector_size) {
                        Some(Ok(used)) => match partition.probe_os() {
                            // Ensure the other OS has at least 5GB of headroom
                            Some(os) => {
                                // The size that the OS partition could be shrunk to
                                let shrink_to = used + (5242880 / sector_size);
                                // The maximum size that we can allocate to the new
                                // install
                                let install_size = partition.sectors() - shrink_to;
                                // Whether there is enough room or not in the install.
                                if install_size > required_space {
                                    Some(InstallOption {
                                        install_size,
                                        kind: InstallKind::Os {
                                            path: partition.get_device_path().into(),
                                            os,
                                            shrink_to,
                                        },
                                    })
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
                                    Some(InstallOption {
                                        install_size,
                                        kind: InstallKind::Partition {
                                            path: partition.get_device_path().into(),
                                            shrink_to,
                                        },
                                    })
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
                    _ => {
                        if partition.sectors() > required_space {
                            Some(InstallOption {
                                install_size: partition.sectors(),
                                kind:         InstallKind::Overwrite {
                                    path: partition.get_device_path().into(),
                                },
                            })
                        } else {
                            None
                        }
                    }
                }
            })
            .collect::<Vec<InstallOption>>();

        let (available, largest): (u64, usize) =
            options.iter().enumerate().fold((0, 0), |(x, y), (id, p)| {
                if p.install_size > x {
                    (p.install_size, id)
                } else {
                    (x, y)
                }
            });

        Ok(InstallOptions {
            options,
            largest_available: available,
            largest_option: largest,
            efi_partitions,
        })
    }
}

#[derive(Debug)]
pub struct InstallOption {
    pub install_size: u64,
    pub kind:         InstallKind,
}

#[derive(Debug)]
pub enum InstallKind {
    Os {
        path:      PathBuf,
        os:        String,
        shrink_to: u64,
    },
    Partition {
        path:      PathBuf,
        shrink_to: u64,
    },
    Overwrite {
        path: PathBuf,
    },
}
