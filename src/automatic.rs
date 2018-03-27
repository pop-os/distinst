use super::{Bootloader, DiskExt, Disks, FileSystemType, PartitionBuilder, PartitionError, PartitionInfo};
use FileSystemType::*;
use libparted::PartitionFlag;
use std::io;
use std::path::{Path, PathBuf};

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
                            Some((os, home)) => {
                                // The size that the OS partition could be shrunk to
                                let shrink_to = used + (5242880 / sector_size);
                                // The maximum size that we can allocate to the new
                                // install
                                let install_size = partition.sectors() - shrink_to;
                                // Whether there is enough room or not in the install.
                                if install_size > required_space {
                                    let path: PathBuf = partition.get_device_path().into();
                                    let home = home.unwrap_or_else(|| path.clone());
                                    Some(InstallOption {
                                        install_size,
                                        path,
                                        kind: InstallKind::AlongsideOS {
                                            home,
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
                                        path: partition.get_device_path().into(),
                                        kind: InstallKind::Partition { shrink_to },
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
                                path:         partition.get_device_path().into(),
                                kind:         InstallKind::Overwrite,
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

pub enum AutomaticError {
    PartitionNotFound,
    Partition { why: PartitionError },
}

#[derive(Debug)]
pub struct InstallOption {
    pub install_size: u64,
    pub path:         PathBuf,
    pub kind:         InstallKind,
}

impl InstallOption {
    pub fn apply(&mut self, disks: &mut Disks, efi: Option<&Path>) -> Result<(), AutomaticError> {
        // Shrinks the partition at the given path, then
        // creates a root partition within the new empty space.
        fn shrink_and_insert(
            disks: &mut Disks,
            path: &Path,
            shrink_to: u64,
            install_size: u64,
        ) -> Result<(), AutomaticError> {
            let (root_device, start_sector) = {
                let (device, existing) = disks
                    .find_partition_mut(path)
                    .ok_or(AutomaticError::PartitionNotFound)?;

                existing
                    .shrink_to(shrink_to)
                    .map_err(|why| AutomaticError::Partition { why })?;

                (device, existing.end_sector + 1)
            };

            let end_sector = start_sector + install_size;
            let root_device = disks.find_disk_mut(&root_device).unwrap();
            root_device.push_partition(
                PartitionBuilder::new(start_sector, end_sector, Luks)
                    .mount(PathBuf::from("/"))
                    .build(),
            );

            Ok(())
        }

        match self.kind {
            InstallKind::AlongsideOS { shrink_to, .. } => {
                shrink_and_insert(disks, &self.path, shrink_to, self.install_size)?;
            }
            InstallKind::Partition { shrink_to } => {
                shrink_and_insert(disks, &self.path, shrink_to, self.install_size)?;
            }
            InstallKind::Overwrite => {
                let (_device, root) = disks
                    .find_partition_mut(&self.path)
                    .ok_or(AutomaticError::PartitionNotFound)?;
                root.set_mount(PathBuf::from("/"));
                root.format_with(FileSystemType::Ext4);
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum InstallKind {
    AlongsideOS {
        os:        String,
        home:      PathBuf,
        shrink_to: u64,
    },
    Partition {
        shrink_to: u64,
    },
    Overwrite,
}
