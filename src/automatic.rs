use super::{
    Bootloader, Disk, DiskExt, Disks, FileSystemType, LvmEncryption, PartitionBuilder,
    PartitionError,
};
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

fn shrink<F: FnMut(&mut Disk, u64, u64)>(
    disks: &mut Disks,
    path: &Path,
    shrink_to: u64,
    install_size: u64,
    mut configure_unused: F,
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

    // Allow the caller decide what happens with the unused space.
    let end_sector = start_sector + install_size;
    let root_device = disks.find_disk_mut(&root_device).unwrap();
    configure_unused(root_device, start_sector, end_sector);

    Ok(())
}

#[derive(Debug)]
pub struct InstallOption {
    pub install_size: u64,
    pub path:         PathBuf,
    pub kind:         InstallKind,
}

pub struct Config<'a> {
    boot:       Option<&'a Path>,
    encryption: Option<(&'a str, Option<LvmEncryption>)>,
    root_fs:    FileSystemType,
}

fn create_boot_partition(disk: &mut Disk, is_efi: bool, start: u64, end: u64) {
    disk.push_partition(
        PartitionBuilder::new(start, end, Fat32)
            .mount(PathBuf::from(if is_efi { "/boot/efi" } else { "/boot" }))
            .flag(PartitionFlag::PED_PARTITION_ESP)
            .build(),
    );
}

fn create_logical_partition(
    disk: &mut Disk,
    logical: &mut (&str, Option<LvmEncryption>),
    start: u64,
    end: u64,
) {
    disk.push_partition(
        PartitionBuilder::new(start, end, if logical.1.is_some() { Luks } else { Lvm })
            .logical_volume((*logical.0).into(), logical.1.take())
            .build(),
    );
}

impl InstallOption {
    pub fn apply(&mut self, disks: &mut Disks, mut config: Config) -> Result<(), AutomaticError> {
        let is_efi = Bootloader::detect() == Bootloader::Efi;
        let boot_required = is_efi || config.encryption.is_some();
        let create_boot = boot_required && config.boot.is_none();
        let mut logical_length = 0;

        match self.kind {
            InstallKind::AlongsideOS { shrink_to, .. } | InstallKind::Partition { shrink_to } => {
                shrink(
                    disks,
                    &self.path,
                    shrink_to,
                    self.install_size,
                    |disk, mut start, end| {
                        if create_boot {
                            let x = start;
                            start += 1000;
                            create_boot_partition(disk, is_efi, x, start);
                        }

                        match config.encryption.as_mut() {
                            Some(ref mut logical_parameters) => {
                                create_logical_partition(disk, logical_parameters, start, end);
                                logical_length = end - start;
                            }
                            None => {
                                disk.push_partition(
                                    PartitionBuilder::new(start, end, config.root_fs)
                                        .mount(PathBuf::from("/"))
                                        .build(),
                                );
                            }
                        }
                    },
                )?;
            }
            InstallKind::Overwrite => {
                if create_boot {
                    unimplemented!()
                } else {
                    // Don't make changes to the partition table, just format and configure.
                    let (_device, partition) = disks
                        .find_partition_mut(&self.path)
                        .ok_or(AutomaticError::PartitionNotFound)?;

                    match config.encryption.as_mut() {
                        Some(&mut (ref mut group, ref mut encryption)) => {
                            partition.format_with(if encryption.is_some() { Luks } else { Lvm });
                            partition.set_volume_group((*group).into(), encryption.take());
                        }
                        None => {
                            partition.format_with(config.root_fs);
                            partition.set_mount(PathBuf::from("/"));
                        }
                    }
                }
            }
        }

        disks.initialize_volume_groups();
        if let Some((group, _)) = config.encryption {
            let logical_disk = disks.get_logical_device_mut(group).unwrap();
            logical_disk.add_partition(
                PartitionBuilder::new(0, logical_length, config.root_fs).mount(PathBuf::from("/")),
            );
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
