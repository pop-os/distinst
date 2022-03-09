use disk_types::{BlockDeviceExt, FileSystem, PartitionExt, PartitionType, SectorExt};
use libparted::{
    Device, FileSystemType as PedFileSystem, Geometry, Partition as PedPartition, PartitionFlag,
    PartitionType as PedPartitionType,
};
use crate::parted::*;
use std::{
    io,
    path::{Path, PathBuf},
};

/// Defines a new partition to be created on the file system.
#[derive(Debug, SmartDefault, Clone, PartialEq)]
pub struct PartitionCreate {
    /// The location of the disk in the system.
    pub path:         PathBuf,
    /// The start sector that the partition will have.
    pub start_sector: u64,
    /// The end sector that the partition will have.
    pub end_sector:   u64,
    /// Whether the filesystem should be formatted.
    pub format:       bool,
    /// The format that the file system should be formatted to.
    pub file_system:  Option<FileSystem>,
    /// Whether the partition should be primary or logical.
    #[default(PartitionType::Primary)]
    pub kind:         PartitionType,
    /// Flags which should be set on the partition.
    pub flags:        Vec<PartitionFlag>,
    /// Defines the label to apply
    pub label:        Option<String>,
}

impl BlockDeviceExt for PartitionCreate {
    fn get_device_path(&self) -> &Path { &self.path }

    fn get_mount_point(&self) -> &[PathBuf] { &[] }
}

impl PartitionExt for PartitionCreate {
    fn get_file_system(&self) -> Option<FileSystem> { self.file_system }

    fn get_sector_end(&self) -> u64 { self.end_sector }

    fn get_sector_start(&self) -> u64 { self.start_sector }

    fn get_partition_flags(&self) -> &[PartitionFlag] { &self.flags }

    fn get_partition_label(&self) -> Option<&str> { self.label.as_deref() }

    fn get_partition_type(&self) -> PartitionType { self.kind }
}

impl SectorExt for PartitionCreate {
    fn get_sectors(&self) -> u64 {
        self.get_sector_end() - self.get_sector_start()
    }
}

/// Creates a new partition on the device using the info in the `partition` parameter.
/// The partition table should reflect the changes before this function exits.
pub fn create_partition<P>(device: &mut Device, partition: &P) -> io::Result<()>
where
    P: PartitionExt,
{
    // Create a new geometry from the start sector and length of the new partition.
    let length = partition.get_sector_end() - partition.get_sector_start();
    let geometry = Geometry::new(&device, partition.get_sector_start() as i64, length as i64)
        .map_err(|why| io::Error::new(why.kind(), format!("failed to create geometry: {}", why)))?;

    // Convert our internal partition type enum into libparted's variant.
    let part_type = match partition.get_partition_type() {
        PartitionType::Primary => PedPartitionType::PED_PARTITION_NORMAL,
        PartitionType::Logical => PedPartitionType::PED_PARTITION_LOGICAL,
        PartitionType::Extended => PedPartitionType::PED_PARTITION_EXTENDED,
    };

    // Open the disk, create the new partition, and add it to the disk.
    let (start, end) = (geometry.start(), geometry.start() + geometry.length());

    info!("creating new partition with {} sectors: {} - {}", length, start, end);

    let fs_type = partition.get_file_system().and_then(|fs| PedFileSystem::get(fs.into()));

    let mut disk = open_disk(device)?;
    let mut part =
        PedPartition::new(&disk, part_type, fs_type.as_ref(), start, end).map_err(|why| {
            io::Error::new(
                why.kind(),
                format!(
                    "failed to create new partition: {}: {}",
                    partition.get_device_path().display(),
                    why
                ),
            )
        })?;

    for &flag in partition.get_partition_flags() {
        if part.is_flag_available(flag) && part.set_flag(flag, true).is_err() {
            error!("unable to set {:?}", flag);
        }
    }

    if let Some(label) = partition.get_partition_label() {
        if part.set_name(label).is_err() {
            error!("unable to set partition name: {}", label);
        }
    }

    // Add the partition, and commit the changes to the disk.
    let constraint = geometry.exact().expect("exact constraint not found");
    disk.add_partition(&mut part, &constraint).map_err(|why| {
        io::Error::new(
            why.kind(),
            format!(
                "failed to create new partition: {}: {}",
                partition.get_device_path().display(),
                why
            ),
        )
    })?;

    // Attempt to write the new partition to the disk.
    info!(
        "committing new partition ({}:{}) on {}",
        start,
        end,
        partition.get_device_path().display()
    );

    commit(&mut disk)
}
