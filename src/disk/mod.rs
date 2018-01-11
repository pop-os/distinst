mod mounts;
mod operations;
mod serial;

use libparted::{Device, Disk as PedDisk, Partition};
use self::mounts::Mounts;
use self::serial::get_serial_no;
use self::operations::*;
use std::io;
use std::str;
use std::path::{Path, PathBuf};
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub enum DiskError {
    DeviceGet,
    DeviceProbe,
    DiskGet,
    DiskNew,
    EndSectorOverlaps,
    LayoutChanged,
    MountsObtain { why: io::Error },
    PartitionOverlaps,
    SerialGet { why: io::Error },
    StartSectorOverlaps,
}

impl Display for DiskError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use self::DiskError::*;
        match *self {
            DeviceGet => writeln!(f, "unable to get device"),
            DeviceProbe => writeln!(f, "unable to probe for devices"),
            DiskGet => writeln!(f, "unable to find disk"),
            DiskNew => writeln!(f, "unable to open disk"),
            EndSectorOverlaps => writeln!(f, "end sector overlaps"),
            LayoutChanged => writeln!(f, "partition layout on disk has changed"),
            MountsObtain { ref why } => writeln!(f, "unable to get mounts: {}", why),
            PartitionOverlaps => writeln!(f, "partition overlaps"),
            SerialGet { ref why } => writeln!(f, "unable to get serial number of device: {}", why),
            StartSectorOverlaps => writeln!(f, "start sector overlaps"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum FileSystemType {
    Btrfs,
    Exfat,
    Ext2,
    Ext3,
    Ext4,
    Fat16,
    Fat32,
    Swap,
    Xfs,
}

#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionType {
    Primary,
    Logical,
}

#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionTable {
    Msdos,
    Gpt,
}

impl FileSystemType {
    fn from(string: &str) -> Option<FileSystemType> {
        let type_ = match string {
            "btrfs" => FileSystemType::Btrfs,
            "exfat" => FileSystemType::Exfat,
            "ext2" => FileSystemType::Ext2,
            "ext3" => FileSystemType::Ext3,
            "ext4" => FileSystemType::Ext4,
            "fat16" => FileSystemType::Fat16,
            "fat32" => FileSystemType::Fat32,
            "linux-swap(v1)" => FileSystemType::Swap,
            "xfs" => FileSystemType::Xfs,
            _ => return None,
        };
        Some(type_)
    }
}

/// Contains all of the information relevant to a given device.
///
/// # Note
///
/// The `device_path` field may be used for identification of the device in the system.
pub struct Disk {
    /// The model name of the device, assigned by the manufacturer.
    pub model_name: String,
    /// A unique identifier to this disk.
    pub serial: String,
    /// The location in the file system where the block device is located.
    pub device_path: PathBuf,
    /// The size of the disk in bytes.
    pub size: u64,
    /// The size of sectors on the disk.
    pub sector_size: u64,
    /// The type of the device, such as SCSI.
    pub device_type: String,
    /// The partition table may be either **MSDOS** or **GPT**.
    pub table_type: Option<PartitionTable>,
    /// Whether the device is currently in a read-only state.
    pub read_only: bool,
    /// The partitions that are stored on the device.
    pub partitions: Vec<PartitionInfo>,
}

impl Disk {
    fn new(device: &mut Device) -> Result<Disk, DiskError> {
        let model_name = String::from_utf8_lossy(device.model()).into();
        let device_path = device.path().to_owned();
        let serial = get_serial_no(&device_path).map_err(|why| DiskError::SerialGet { why })?;
        let size = device.length();
        let sector_size = device.sector_size();
        let device_type = format!("{:?}", device.type_());
        let read_only = device.read_only();

        // Attempts to open the disk to obtain information regarding the partition table
        // and the partitions stored on the device.
        let disk = PedDisk::new(device).map_err(|_| DiskError::DiskNew)?;

        // Checks whether there is a partition table, and if so, which kind.
        let table_type = disk.get_disk_type_name().and_then(|tn| match tn {
            b"gpt" => Some(PartitionTable::Gpt),
            b"msdos" => Some(PartitionTable::Msdos),
            _ => None,
        });

        // Determines if the table is of the msdos variable, as msdos partition
        // tables do not support partition names.
        let is_msdos = table_type.map_or(false, |tt| tt == PartitionTable::Msdos);

        Ok(Disk {
            model_name,
            device_path,
            serial,
            size,
            sector_size,
            device_type,
            read_only,
            table_type,
            partitions: if table_type.is_some() {
                let mut partitions = Vec::new();
                for part in disk.parts() {
                    let part_result = PartitionInfo::new(&part, is_msdos)
                        .map_err(|why| DiskError::MountsObtain { why })?;
                    if let Some(part) = part_result {
                        partitions.push(part);
                    }
                }
                partitions
            } else {
                Vec::new()
            },
        })
    }

    /// Obtains the disk that corresponds to a given device path.
    ///
    /// The `name` of the device should be a path, such as `/dev/sda`. If the device could
    /// not be found, then `Err(DiskError::DeviceGet)` will be returned.
    pub fn from_name<P: AsRef<Path>>(name: P) -> Result<Disk, DiskError> {
        let mut device = Device::get(name).map_err(|_| DiskError::DeviceGet)?;
        Disk::new(&mut device).map_err(|_| DiskError::DiskNew)
    }

    /// Obtains the disk that corresponds to a given serial number.
    ///
    /// First attempts to check if the supplied name has the valid serial number (highly likely),
    /// then performs a full probe of all disks in the system to attempt to find the matching
    /// serial number, in the event that the user swapped hard drive positions.
    ///
    /// If no match is found, then `Err(DiskError::DeviceGet)` is returned.
    fn from_name_with_serial<P: AsRef<Path>>(name: P, serial: &str) -> Result<Disk, DiskError> {
        Disk::from_name(name).and_then(|source| {
            if &source.serial == serial {
                Ok(source)
            } else {
                Disks::probe_devices().and_then(|disks| {
                    disks
                        .0
                        .into_iter()
                        .find(|disk| &disk.serial == serial)
                        .ok_or(DiskError::DeviceGet)
                })
            }
        })
    }

    pub fn add_partition(&mut self, start: i64, end: i64, fs: FileSystemType) -> Result<(), DiskError> {
        let after = self.sector_overlaps(start).ok_or(DiskError::StartSectorOverlaps)?;
        let before = self.sector_overlaps(end).ok_or(DiskError::EndSectorOverlaps)?;
        if after != before - 1 {
            return Err(DiskError::PartitionOverlaps);
        }

        unimplemented!();

        Ok(())
    }

    fn sector_overlaps(&self, sector: i64) -> Option<usize> {
        unimplemented!();
    }

    /// Returns an error if the other disk does not contain the same source partitions.
    fn validate_layout(&self, other: &Disk) -> Result<(), DiskError> {
        let mut new_parts = other.partitions.iter();
        'outer: for source in &self.partitions {
            'inner: while let Some(new) = new_parts.next() {
                if source.is_same_partition_as(new) {
                    continue 'outer;
                }
            }
            return Err(DiskError::LayoutChanged);
        }

        Ok(())
    }

    fn diff(&self, new: &Disk) -> Result<DiskOps, DiskError> {
        self.validate_layout(new)?;

        let mut remove_partitions = Vec::new();
        let mut change_partitions = Vec::new();
        let mut create_partitions = Vec::new();

        let mut new_parts = new.partitions.iter();
        let mut new_part = None;

        'outer: for source in &self.partitions {
            'inner: loop {
                let mut next_part = new_part.take().or(new_parts.next());
                if let Some(new) = next_part {
                    if new.is_source {
                        if source.number != new.number {
                            unreachable!("layout validation");
                        }

                        if new.remove {
                            remove_partitions.push(new.number);
                            continue 'outer;
                        }

                        if source.requires_changes(new) {
                            change_partitions.push(PartitionChange {
                                num: source.number,
                                start: new.start_sector,
                                end: new.end_sector,
                                format: new.filesystem
                            });
                        }

                        continue 'outer;
                    } else {
                        create_partitions.push(PartitionCreate {
                            start_sector: new.start_sector,
                            end_sector: new.end_sector,
                            file_system: new.filesystem.unwrap(),
                        });

                        continue 'inner;
                    }
                }
            }
        }

        Ok(DiskOps { remove_partitions, change_partitions, create_partitions })
    }

    pub fn commit(&self) -> Result<(), DiskError> {
        let source = Disk::from_name_with_serial(&self.device_path, &self.serial)?;
        unimplemented!();
    }

    pub fn path(&self) -> &Path {
        &self.device_path
    }
}

pub struct PartitionInfo {
    is_source: bool,
    remove: bool,
    pub part_type: PartitionType,
    pub filesystem: Option<FileSystemType>,
    pub number: i32,
    pub name: Option<String>,
    pub device_path: PathBuf,
    pub mount_point: Option<PathBuf>,
    pub active: bool,
    pub busy: bool,
    pub start_sector: i64,
    pub end_sector: i64,
}

impl PartitionInfo {
    pub fn new(partition: &Partition, is_msdos: bool) -> io::Result<Option<PartitionInfo>> {
        let device_path = partition.get_path().unwrap().to_path_buf();
        let mounts = Mounts::new()?;

        Ok(Some(PartitionInfo {
            is_source: true,
            remove: false,
            part_type: match partition.type_get_name() {
                "primary" => PartitionType::Primary,
                "logical" => PartitionType::Logical,
                _ => return Ok(None),
            },
            mount_point: mounts.get_mount_point(&device_path),
            filesystem: partition.fs_type_name().and_then(FileSystemType::from),
            number: partition.num(),
            name: if is_msdos {
                None
            } else {
                partition.name().map(String::from)
            },
            // Note that primary and logical partitions should always have a path.
            device_path,
            active: partition.is_active(),
            busy: partition.is_busy(),
            start_sector: partition.geom_start(),
            end_sector: partition.geom_end(),
        }))
    }

    pub fn is_swap(&self) -> bool {
        self.filesystem
            .map_or(false, |fs| fs == FileSystemType::Swap)
    }

    pub fn path(&self) -> &Path {
        &self.device_path
    }

    fn requires_changes(&self, other: &PartitionInfo) -> bool {
        self.sectors_differ_from(other) || self.filesystem != other.filesystem
    }

    fn sectors_differ_from(&self, other: &PartitionInfo) -> bool {
        self.start_sector != other.start_sector || self.end_sector != other.end_sector
    }

    fn is_same_partition_as(&self, other: &PartitionInfo) -> bool {
        self.is_source && other.is_source && self.number == other.number
    }
}

pub struct Disks(Vec<Disk>);

impl Disks {
    pub fn probe_devices() -> Result<Disks, DiskError> {
        let mut output: Vec<Disk> = Vec::new();
        for device_result in Device::devices(true) {
            let mut device = device_result.map_err(|_| DiskError::DeviceProbe)?;
            output.push(Disk::new(&mut device)?);
        }

        Ok(Disks(output))
    }
}
