mod mounts;
mod serial;

use libparted::{Device, Disk as PedDisk, Partition};
use self::mounts::Mounts;
use self::serial::get_serial_no;
use std::io;
use std::str;
use std::path::{Path, PathBuf};
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub enum DiskError {
    DeviceGet,
    DeviceProbe,
    DiskNew,
    DiskGet,
    MountsObtain { why: io::Error },
    SerialGet { why: io::Error },
}

impl Display for DiskError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use self::DiskError::*;
        match *self {
            DeviceGet => writeln!(f, "unable to get device"),
            DeviceProbe => writeln!(f, "unable to probe for devices"),
            DiskNew => writeln!(f, "unable to open disk"),
            DiskGet => writeln!(f, "unable to find disk"),
            MountsObtain { ref why } => writeln!(f, "unable to get mounts: {}", why),
            SerialGet { ref why } => writeln!(f, "unable to get serial number of device: {}", why),
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

    pub fn path(&self) -> &Path {
        &self.device_path
    }
}

pub struct PartitionInfo {
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
