//! Contains source code for applying physical disk operations to disks.

use super::*;
use disk_types::{FileSystem, PartitionTable, PartitionType};
use external::{blockdev, mkfs};
use libparted::{Device, Disk as PedDisk, Partition as PedPartition};
use mkpart::PartitionCreate;
use parted::*;
use rayon::prelude::*;
use resize::PartitionChange;
use std::{
    io,
    path::{Path, PathBuf},
};

/// Obtains a partition from the disk by its ID.
pub fn get_partition<'a>(disk: &'a mut PedDisk, part: u32) -> io::Result<PedPartition<'a>> {
    disk.get_partition(part).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("partition {} was not found on {}", part, unsafe {
                disk.get_device().path().display()
            }),
        )
    })
}

/// The first state of disk operations, which provides a method for removing
/// partitions.
#[derive(Debug, Clone, PartialEq)]
pub struct DiskOps<'a> {
    pub mklabel:           Option<PartitionTable>,
    pub device_path:       &'a Path,
    pub remove_partitions: Vec<u64>,
    pub change_partitions: Vec<PartitionChange>,
    pub create_partitions: Vec<PartitionCreate>,
}

impl<'a> DiskOps<'a> {
    /// Useful for knowing when operations should be attempted.
    pub fn is_empty(&self) -> bool {
        self.remove_partitions.is_empty()
            && self.change_partitions.is_empty()
            && self.create_partitions.is_empty()
    }

    /// The first stage of disk operations, where a new partition table may be
    /// generated
    pub fn remove(self) -> io::Result<ChangePartitions<'a>> {
        info!("{}: executing remove operations", self.device_path.display(),);

        if let Some(table) = self.mklabel {
            mklabel(self.device_path, table)?;
        }

        let mut device = open_device(self.device_path)?;

        {
            let mut disk = open_disk(&mut device)?;
            let mut changes_required = false;
            for partition in self.remove_partitions {
                remove_partition_by_sector(&mut disk, partition)?;
                changes_required = true;
            }

            if changes_required {
                info!("attempting to remove partitions from {}", self.device_path.display());
                commit(&mut disk)?;
                info!("successfully removed partitions from {}", self.device_path.display());
            }
        }

        sync(&mut device)?;
        Ok(ChangePartitions {
            device_path:       self.device_path,
            change_partitions: self.change_partitions,
            create_partitions: self.create_partitions,
        })
    }
}

/// The second state of disk operations, which provides a method for changing
/// partitions.
pub struct ChangePartitions<'a> {
    device_path:       &'a Path,
    change_partitions: Vec<PartitionChange>,
    create_partitions: Vec<PartitionCreate>,
}

impl<'a> ChangePartitions<'a> {
    /// The second stage of disk operations, where existing partitions will be
    /// modified.
    pub fn change(self) -> io::Result<CreatePartitions<'a>> {
        info!("{}: executing change operations", self.device_path.display(),);

        let mut device = open_device(self.device_path)?;
        let mut resize_partitions = Vec::new();

        for change in &self.change_partitions {
            let sector_size = device.sector_size();
            let mut disk = open_disk(&mut device)?;
            let mut flags_changed = false;
            let mut name_changed = false;

            {
                // Obtain the partition that needs to be changed by its ID.
                let mut part = get_partition(&mut disk, change.num as u32)?;

                for flag in &change.flags {
                    if part.is_flag_available(*flag) {
                        match part.set_flag(*flag, true) {
                            Ok(()) => flags_changed = true,
                            Err(_) => {
                                error!(
                                    "unable to set {:?} for {}{}",
                                    flag,
                                    self.device_path.display(),
                                    change.num
                                );
                            }
                        }
                    }
                }

                if let Some(ref label) = change.label {
                    name_changed = true;
                    if part.set_name(label).is_err() {
                        error!("unable to set partition name: {}", label);
                    }
                }

                // For convenience, grab the start and end sectors from the partition's geom.
                let (start, end) = (part.geom_start() as u64, part.geom_end() as u64);

                // If the partition needs to be resized/moved, this will execute.
                if end != change.end || start != change.start {
                    info!("{} will be resized", change.path.display());
                    resize_partitions.push((
                        change.clone(),
                        ResizeOperation::new(
                            sector_size,
                            BlockCoordinates::new(start, end),
                            BlockCoordinates::new(change.start, change.end),
                        ),
                    ));
                    continue;
                }
            }

            if flags_changed || name_changed {
                if name_changed {
                    info!("renaming {}", change.num);
                }

                commit(&mut disk)?;
            }
        }

        // Flush the OS cache and drop the device before proceeding to formatting.
        sync(&mut device)?;

        // TODO: Maybe not require a raw pointer here?
        let device = &mut device as *mut Device;
        for (change, resize_op) in resize_partitions {
            transform(
                change,
                resize_op,
                // This is the delete function.
                |partition| {
                    let mut disk = open_disk(unsafe { &mut (*device) })?;
                    remove_partition_by_number(&mut disk, partition)?;
                    commit(&mut disk)?;

                    Ok(())
                },
                // And this is the partition-creation function
                |start, end, fs, flags, label, kind| {
                    create_partition(
                        unsafe { &mut (*device) },
                        &PartitionCreate {
                            path: self.device_path.to_path_buf(),
                            start_sector: start,
                            end_sector: end,
                            format: false,
                            file_system: fs,
                            kind,
                            flags,
                            label,
                        },
                    )?;

                    let res = get_partition_id_and_path(self.device_path, start as i64)?;
                    Ok(res)
                },
            )?;
        }

        // Proceed to the next state in the machine.
        Ok(CreatePartitions {
            device_path:       self.device_path,
            create_partitions: self.create_partitions,
            format_partitions: Vec::new(),
        })
    }
}

/// The partition creation stage of disk operations, which provides a method
/// for creating new partitions.
pub struct CreatePartitions<'a> {
    device_path:       &'a Path,
    create_partitions: Vec<PartitionCreate>,
    format_partitions: Vec<(PathBuf, FileSystem)>,
}

impl<'a> CreatePartitions<'a> {
    /// If any new partitions were specified, they will be created here.
    pub fn create(mut self) -> io::Result<FormatPartitions> {
        info!("{}: executing creation operations", self.device_path.display(),);

        for partition in &self.create_partitions {
            info!("creating partition ({:?}) on {}", partition, self.device_path.display());

            {
                let mut device = open_device(self.device_path)?;
                create_partition(&mut device, partition)?;
                sync(&mut device)?;
            }

            if partition.kind != PartitionType::Extended {
                // Open a second instance of the disk which we need to get the new partition ID.
                let path = get_partition_id(self.device_path, partition.start_sector as i64)?;
                self.format_partitions.push((
                    path,
                    partition
                        .file_system
                        .expect("file system does not exist when creating partition"),
                ));
            }
        }

        // Attempt to sync three times before returning an error.
        for attempt in 0..3 {
            ::std::thread::sleep(::std::time::Duration::from_secs(1));
            let result = blockdev(self.device_path, &["--flushbufs", "--rereadpt"]);
            if result.is_err() && attempt == 2 {
                result.map_err(|why| {
                    io::Error::new(why.kind(), format!("failed to synchronize disk: {}", why))
                })?
            } else {
                break;
            }
        }

        Ok(FormatPartitions(self.format_partitions))
    }
}

pub fn get_partition_and<T, F: FnOnce(PedPartition) -> T>(
    path: &Path,
    start_sector: i64,
    action: F,
) -> io::Result<T> {
    let mut device = get_device(path)?;
    let disk = open_disk(&mut device)?;
    let result = disk
        .get_partition_by_sector(start_sector)
        .map(action)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "partition not found"))?;

    Ok(result)
}

pub fn get_partition_id(path: &Path, start_sector: i64) -> io::Result<PathBuf> {
    get_partition_and(path, start_sector, |part| {
        part.get_path().expect("ped partition does not have path").to_path_buf()
    })
}

pub fn get_partition_id_and_path(path: &Path, start_sector: i64) -> io::Result<(i32, PathBuf)> {
    get_partition_and(path, start_sector, |part| {
        (part.num(), part.get_path().expect("ped partition does not have path").to_path_buf())
    })
}

/// The final stage of disk operations, where all partitions to be formatted can be
/// formatted in parallel.
pub struct FormatPartitions(pub Vec<(PathBuf, FileSystem)>);

impl FormatPartitions {
    /// Finally, format all of the modified and created partitions.
    pub fn format(self) -> io::Result<()> {
        info!("executing format operations");
        self.0
            .par_iter()
            .try_for_each(|&(ref part, fs)| {
                info!("formatting {} with {:?}", part.display(), fs);
                mkfs(part, fs).map_err(|why| {
                    io::Error::new(
                        why.kind(),
                        format!("failed to format {} with {}: {}", part.display(), fs, why),
                    )
                })
            })
    }
}
