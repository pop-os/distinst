use super::*;
use super::external::mkfs;
use super::resize::{transform, Coordinates, ResizeOperation};
use blockdev;
use libparted::{
    Disk as PedDisk, FileSystemType as PedFileSystemType, Geometry, Partition as PedPartition,
    PartitionFlag, PartitionType as PedPartitionType,
};
use std::path::Path;

/// Removes a partition by its ID from the disk.
fn remove_partition(disk: &mut PedDisk, partition: u32) -> Result<(), DiskError> {
    info!(
        "libdistinst: removing partition {} on {}",
        partition,
        unsafe { disk.get_device().path().display() }
    );
    disk.remove_partition(partition)
        .map_err(|why| DiskError::PartitionRemove {
            partition: partition as i32,
            why,
        })
}

/// Obtains a partition from the disk by its ID.
fn get_partition<'a>(disk: &'a mut PedDisk, part: u32) -> Result<PedPartition<'a>, DiskError> {
    info!("libdistinst: getting partition {} on {}", part, unsafe {
        disk.get_device().path().display()
    });
    disk.get_partition(part)
        .ok_or(DiskError::PartitionNotFound {
            partition: part as i32,
        })
}

/// Writes a new partition table to the disk, clobbering it in the process.
fn mklabel<P: AsRef<Path>>(device_path: P, kind: PartitionTable) -> Result<(), DiskError> {
    info!(
        "libdistinst: writing {:?} table on {}",
        kind,
        device_path.as_ref().display()
    );
    open_device(&device_path).and_then(|mut device| {
        let kind = match kind {
            PartitionTable::Gpt => PedDiskType::get("gpt").unwrap(),
            PartitionTable::Msdos => PedDiskType::get("msdos").unwrap(),
        };

        PedDisk::new_fresh(&mut device, kind)
            .map_err(|why| DiskError::DiskFresh { why })
            .and_then(|mut disk| {
                commit(&mut disk).and_then(|_| sync(&mut unsafe { disk.get_device() }))
            })
    })?;

    Ok(())
}

/// The first state of disk operations, which provides a method for removing partitions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DiskOps<'a> {
    pub(crate) mklabel:           Option<PartitionTable>,
    pub(crate) device_path:       &'a Path,
    pub(crate) remove_partitions: Vec<i32>,
    pub(crate) change_partitions: Vec<PartitionChange>,
    pub(crate) create_partitions: Vec<PartitionCreate>,
}

impl<'a> DiskOps<'a> {
    /// The first stage of disk operations, where a new partition table may be generated
    pub(crate) fn remove(self) -> Result<ChangePartitions<'a>, DiskError> {
        info!(
            "libdistinst: {}: executing remove operations",
            self.device_path.display(),
        );

        if let Some(table) = self.mklabel {
            mklabel(self.device_path, table)?;
        }

        let mut device = open_device(self.device_path)?;

        {
            let mut disk = open_disk(&mut device)?;
            let mut changes_required = false;
            for partition in self.remove_partitions {
                remove_partition(&mut disk, partition as u32)?;
                changes_required = true;
            }

            if changes_required {
                info!(
                    "libdistinst: attempting to remove partitions from {}",
                    self.device_path.display()
                );
                commit(&mut disk)?;
                info!(
                    "libdistinst: successfully removed partitions from {}",
                    self.device_path.display()
                );
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

/// The second state of disk operations, which provides a method for changing partitions.
pub(crate) struct ChangePartitions<'a> {
    device_path:       &'a Path,
    change_partitions: Vec<PartitionChange>,
    create_partitions: Vec<PartitionCreate>,
}

impl<'a> ChangePartitions<'a> {
    /// The second stage of disk operations, where existing partitions will be modified.
    pub(crate) fn change(self) -> Result<CreatePartitions<'a>, DiskError> {
        info!(
            "libdistinst: {}: executing change operations",
            self.device_path.display(),
        );

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

                // For convenience, grab the start and env sectors from the partition's geom.
                let (start, end) = (part.geom_start() as u64, part.geom_end() as u64);

                // If the partition needs to be resized/moved, this will execute.
                if end != change.end || start != change.start {
                    info!("libdistinst: {} will be resized", change.path.display());
                    resize_partitions.push((
                        change.clone(),
                        ResizeOperation::new(
                            sector_size,
                            Coordinates::new(start, end),
                            Coordinates::new(change.start, change.end),
                        ),
                    ));
                    continue;
                }
            }

            if flags_changed || name_changed {
                if name_changed {
                    info!("libdistinst: renaming {}", change.num);
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
                    open_disk(unsafe { &mut (*device) }).and_then(|mut disk| {
                        remove_partition(&mut disk, partition).and_then(|_| commit(&mut disk))
                    })
                },
                // And this is the partition-creation function
                // TODO: label & partition kind support
                |start, end, fs, flags| {
                    create_partition(
                        unsafe { &mut (*device) },
                        &PartitionCreate {
                            path:         self.device_path.to_path_buf(),
                            start_sector: start,
                            end_sector:   end,
                            format:       false,
                            file_system:  fs,
                            kind:         PartitionType::Primary,
                            flags:        Vec::from(flags),
                            label:        None,
                        },
                    )?;

                    get_partition_id_and_path(self.device_path, start as i64)
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
pub(crate) struct CreatePartitions<'a> {
    device_path:       &'a Path,
    create_partitions: Vec<PartitionCreate>,
    format_partitions: Vec<(PathBuf, FileSystemType)>,
}

impl<'a> CreatePartitions<'a> {
    /// If any new partitions were specified, they will be created here.
    pub(crate) fn create(mut self) -> Result<FormatPartitions, DiskError> {
        info!(
            "libdistinst: {}: executing creation operations",
            self.device_path.display(),
        );

        for partition in &self.create_partitions {
            info!(
                "libdistinst: creating partition ({:?}) on {}",
                partition,
                self.device_path.display()
            );
            {
                let mut device = open_device(self.device_path)?;
                create_partition(&mut device, partition)?;
                sync(&mut device)?;
            }

            // Open a second instance of the disk which we need to get the new partition ID.
            let path = get_partition_id(self.device_path, partition.start_sector as i64)?;
            self.format_partitions
                .push((path, partition.file_system.unwrap()));
        }

        // Attempt to sync three times before returning an error.
        for attempt in 0..3 {
            ::std::thread::sleep(::std::time::Duration::from_secs(1));
            let result = blockdev(self.device_path, &["--flushbufs", "--rereadpt"]);
            if result.is_err() && attempt == 2 {
                result.map_err(|why| DiskError::DiskSync { why })?
            } else {
                break;
            }
        }

        Ok(FormatPartitions(self.format_partitions))
    }
}

/// Creates a new partition on the device using the info in the `partition` parameter.
/// The partition table should reflect the changes before this function exits.
fn create_partition(device: &mut Device, partition: &PartitionCreate) -> Result<(), DiskError> {
    // Create a new geometry from the start sector and length of the new partition.
    let length = partition.end_sector - partition.start_sector;
    let geometry = Geometry::new(&device, partition.start_sector as i64, length as i64)
        .map_err(|why| DiskError::GeometryCreate { why })?;

    // Convert our internal partition type enum into libparted's variant.
    let part_type = match partition.kind {
        PartitionType::Primary => PedPartitionType::PED_PARTITION_NORMAL,
        PartitionType::Logical => PedPartitionType::PED_PARTITION_LOGICAL,
    };

    // Open the disk, create the new partition, and add it to the disk.
    let (start, end) = (geometry.start(), geometry.start() + geometry.length());

    info!(
        "libdistinst: creating new partition with {} sectors: {} - {}",
        length, start, end
    );

    let fs_type = partition
        .file_system
        .map(|fs| PedFileSystemType::get(fs.into()).unwrap());

    let mut disk = open_disk(device)?;
    let mut part = PedPartition::new(&disk, part_type, fs_type.as_ref(), start, end)
        .map_err(|why| DiskError::PartitionCreate { why })?;

    for &flag in &partition.flags {
        if part.is_flag_available(flag) && part.set_flag(flag, true).is_err() {
            error!("unable to set {:?}", flag);
        }
    }

    if let Some(ref label) = partition.label {
        if part.set_name(label).is_err() {
            error!("unable to set partition name: {}", label);
        }
    }

    // Add the partition, and commit the changes to the disk.
    let constraint = geometry.exact().unwrap();
    disk.add_partition(&mut part, &constraint)
        .map_err(|why| DiskError::PartitionCreate { why })?;

    // Attempt to write the new partition to the disk.
    info!(
        "libdistinst: committing new partition ({}:{}) on {}",
        start,
        end,
        partition.path.display()
    );

    commit(&mut disk)
}

fn get_partition_and<T, F: FnOnce(PedPartition) -> T>(
    path: &Path,
    start_sector: i64,
    action: F,
) -> Result<T, DiskError> {
    get_device(path).and_then(|mut device| {
        open_disk(&mut device).and_then(|disk| {
            disk.get_partition_by_sector(start_sector)
                .map(action)
                .ok_or(DiskError::NewPartNotFound)
        })
    })
}

fn get_partition_id(path: &Path, start_sector: i64) -> Result<PathBuf, DiskError> {
    get_partition_and(path, start_sector, |part| {
        part.get_path().unwrap().to_path_buf()
    })
}

fn get_partition_id_and_path(path: &Path, start_sector: i64) -> Result<(i32, PathBuf), DiskError> {
    get_partition_and(path, start_sector, |part| {
        (part.num(), part.get_path().unwrap().to_path_buf())
    })
}

pub struct FormatPartitions(Vec<(PathBuf, FileSystemType)>);

impl FormatPartitions {
    // Finally, format all of the modified and created partitions.
    pub fn format(self) -> Result<(), DiskError> {
        info!("libdistinst: executing format operations");
        for (part, fs) in self.0 {
            info!("libdistinst: formatting {} with {:?}", part.display(), fs);
            mkfs(&part, fs).map_err(|why| DiskError::PartitionFormat { why })?;
        }
        Ok(())
    }
}

/// Defines the move and resize operations that the partition with this number
/// will need to perform.
///
/// If the `start` sector differs from the source, the partition will be moved.
/// If the `end` minus the `start` differs from the length of the source, the
/// partition will be resized. Once partitions have been moved and resized,
/// they will be formatted accordingly, if formatting was set.
///
/// # Note
///
/// Resize operations should always be performed before move operations.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartitionChange {
    /// The location of the device where the partition resides.
    pub(crate) device_path: PathBuf,
    /// The location of the partition in the system.
    pub(crate) path: PathBuf,
    /// The partition ID that will be changed.
    pub(crate) num: i32,
    /// The start sector that the partition will have.
    pub(crate) start: u64,
    /// The end sector that the partition will have.
    pub(crate) end: u64,
    /// Required information if the partition will be moved.
    pub(crate) sector_size: u64,
    /// The file system that is currently on the partition.
    pub(crate) filesystem: Option<FileSystemType>,
    /// A diff of flags which should be set on the partition.
    pub(crate) flags: Vec<PartitionFlag>,
    /// All of the flags that are set on the new disk.
    pub(crate) new_flags: Vec<PartitionFlag>,
    /// Defines the label to apply
    pub(crate) label: Option<String>,
}

/// Defstart, end, fs, flagsines a new partition to be created on the file system.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartitionCreate {
    /// The location of the disk in the system.
    pub(crate) path: PathBuf,
    /// The start sector that the partition will have.
    pub(crate) start_sector: u64,
    /// The end sector that the partition will have.
    pub(crate) end_sector: u64,
    /// Whether the filesystem should be formatted.
    pub(crate) format: bool,
    /// The format that the file system should be formatted to.
    pub(crate) file_system: Option<FileSystemType>,
    /// Whether the partition should be primary or logical.
    pub(crate) kind: PartitionType,
    /// Flags which should be set on the partition.
    pub(crate) flags: Vec<PartitionFlag>,
    /// Defines the label to apply
    pub(crate) label: Option<String>,
}
