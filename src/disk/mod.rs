pub(crate) mod external;
pub mod mount;

mod disk;
mod error;
mod lvm;
mod mounts;
mod operations;
mod partitions;
mod sector;
mod serial;
mod swaps;

pub use self::disk::DiskExt;
pub use self::error::{DiskError, PartitionSizeError};
pub use self::lvm::{LvmDevice, LvmEncryption};
pub use self::partitions::{check_partition_size, FileSystemType, PartitionBuilder, PartitionInfo, PartitionType};
pub use self::sector::Sector;
pub use self::swaps::Swaps;
pub use libparted::PartitionFlag;

use self::disk::find_partition;
use self::external::{cryptsetup_close, deactivate_volumes, lvs, pvremove, pvs, vgremove};
use self::mount::{swapoff, umount};
use self::mounts::Mounts;
use self::operations::*;
use self::serial::get_serial;
use libparted::{Device, DeviceType, Disk as PedDisk, DiskType as PedDiskType};

use itertools::Itertools;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::read_dir;
use std::io;
use std::iter::{self, FromIterator};
use std::path::{Path, PathBuf};
use std::str;

/// Bootloader type
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Bootloader {
    Bios,
    Efi,
}

impl Bootloader {
    /// Detects whether the system is running from EFI.
    pub fn detect() -> Bootloader {
        if Path::new("/sys/firmware/efi").is_dir() {
            Bootloader::Efi
        } else {
            Bootloader::Bios
        }
    }
}

/// Specifies whether the partition table on the disk is **MSDOS** or **GPT**.
#[derive(Debug, PartialEq, Clone, Copy, Hash)]
pub enum PartitionTable {
    Msdos,
    Gpt,
}

/// Gets a `libparted::Device` from the given name.
fn get_device<'a, P: AsRef<Path>>(name: P) -> Result<Device<'a>, DiskError> {
    info!("libdistinst: getting device at {}", name.as_ref().display());
    Device::get(name).map_err(|why| DiskError::DeviceGet { why })
}

/// Gets and opens a `libparted::Device` from the given name.
fn open_device<'a, P: AsRef<Path>>(name: P) -> Result<Device<'a>, DiskError> {
    info!("libdistinst: opening device at {}", name.as_ref().display());
    Device::new(name).map_err(|why| DiskError::DeviceGet { why })
}

/// Opens a `libparted::Disk` from a `libparted::Device`.
fn open_disk<'a>(device: &'a mut Device) -> Result<PedDisk<'a>, DiskError> {
    info!("libdistinst: opening disk at {}", device.path().display());
    let device = device as *mut Device;
    unsafe {
        match PedDisk::new(&mut *device) {
            Ok(disk) => Ok(disk),
            Err(_) => {
                info!("libdistinst: unable to open disk; creating new table on it");
                PedDisk::new_fresh(
                    &mut *device,
                    match Bootloader::detect() {
                        Bootloader::Bios => PedDiskType::get("msdos").unwrap(),
                        Bootloader::Efi => PedDiskType::get("gpt").unwrap(),
                    },
                ).map_err(|why| DiskError::DiskNew { why })
            }
        }
    }
}

/// Attempts to commit changes to the disk, return a `DiskError` on failure.
fn commit(disk: &mut PedDisk) -> Result<(), DiskError> {
    info!("libdistinst: commiting changes to {}", unsafe {
        disk.get_device().path().display()
    });
    disk.commit().map_err(|why| DiskError::DiskCommit { why })
}

/// Flushes the OS cache, return a `DiskError` on failure.
fn sync(device: &mut Device) -> Result<(), DiskError> {
    info!("libdistinst: syncing device at {}", device.path().display());
    device.sync().map_err(|why| DiskError::DiskSync { why })
}

/// Contains all of the information relevant to a given device.
///
/// # Note
///
/// The `device_path` field may be used for identification of the device in the system.
#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    /// The model name of the device, assigned by the manufacturer.
    pub(crate) model_name: String,
    /// A unique identifier to this disk.
    pub(crate) serial: String,
    /// The location in the file system where the block device is located.
    pub(crate) device_path: PathBuf,
    /// The size of the disk in sectors.
    pub(crate) size: u64,
    /// The size of sectors on the disk.
    pub(crate) sector_size: u64,
    /// The type of the device, such as SCSI.
    pub(crate) device_type: String,
    /// The partition table may be either **MSDOS** or **GPT**.
    pub(crate) table_type: Option<PartitionTable>,
    /// Whether the device is currently in a read-only state.
    pub(crate) read_only: bool,
    /// Defines whether the device should be wiped or not. The `table_type`
    /// field will be used to determine which table to write to the disk.
    pub(crate) mklabel: bool,
    /// The partitions that are stored on the device.
    pub(crate) partitions: Vec<PartitionInfo>,
}

impl DiskExt for Disk {
    fn get_table_type(&self) -> Option<PartitionTable> { self.table_type }

    fn get_sectors(&self) -> u64 { self.size }

    fn get_sector_size(&self) -> u64 { self.sector_size }

    fn get_partitions(&self) -> &[PartitionInfo] { &self.partitions }

    fn get_partitions_mut(&mut self) -> &mut [PartitionInfo] { &mut self.partitions }

    fn get_device_path(&self) -> &Path { &self.device_path }

    fn validate_partition_table(&self, part_type: PartitionType) -> Result<(), DiskError> {
        match self.table_type {
            Some(PartitionTable::Gpt) => (),
            Some(PartitionTable::Msdos) => {
                let (primary, logical) = self.get_partition_type_count();
                if part_type == PartitionType::Primary {
                    if primary == 4 || (primary == 3 && logical != 0) {
                        return Err(DiskError::PrimaryPartitionsExceeded);
                    }
                } else if primary == 4 {
                    return Err(DiskError::PrimaryPartitionsExceeded);
                }
            }
            None => return Err(DiskError::PartitionTableNotFound),
        }

        Ok(())
    }

    fn push_partition(&mut self, partition: PartitionInfo) { self.partitions.push(partition); }
}

impl Disk {
    fn new(device: &mut Device) -> Result<Disk, DiskError> {
        info!(
            "libdistinst: obtaining disk information from {}",
            device.path().display()
        );
        let model_name = device.model().into();
        let device_path = device.path().to_owned();
        let serial = match device.type_() {
            // Encrypted devices do not have serials
            DeviceType::PED_DEVICE_DM | DeviceType::PED_DEVICE_LOOP => "".into(),
            _ => get_serial(&device_path).map_err(|why| DiskError::SerialGet { why })?,
        };
        let size = device.length();
        let sector_size = device.sector_size();
        let device_type = format!("{:?}", device.type_());
        let read_only = device.read_only();

        // Attempts to open the disk to obtain information regarding the partition table
        // and the partitions stored on the device.
        let disk = open_disk(device)?;

        // Checks whether there is a partition table, and if so, which kind.
        let table_type = disk.get_disk_type_name().and_then(|tn| match tn {
            "gpt" => Some(PartitionTable::Gpt),
            "msdos" => Some(PartitionTable::Msdos),
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
            mklabel: false,
            partitions: if table_type.is_some() {
                let mut partitions = Vec::new();
                for part in disk.parts() {
                    // skip invalid partitions (metadata / free)
                    if part.num() == -1 {
                        continue;
                    }

                    // grab partition info results
                    let part_result = PartitionInfo::new_from_ped(&part, is_msdos)
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
        get_device(name).and_then(|mut device| Disk::new(&mut device))
    }

    /// Obtains the disk that corresponds to a given serial model.
    ///
    /// First attempts to check if the supplied name has the valid serial number (highly likely),
    /// then performs a full probe of all disks in the system to attempt to find the matching
    /// serial number, in the event that the user swapped hard drive positions.
    ///
    /// If no match is found, then `Err(DiskError::DeviceGet)` is returned.
    fn from_name_with_serial<P: AsRef<Path>>(name: P, serial: &str) -> Result<Disk, DiskError> {
        Disk::from_name(name).and_then(|source| {
            if source.serial == serial {
                Ok(source)
            } else {
                // Attempt to find the serial model on another disk.
                Disks::probe_devices().and_then(|disks| {
                    disks
                        .physical
                        .into_iter()
                        .find(|disk| disk.serial == serial)
                        .ok_or(DiskError::InvalidSerial)
                })
            }
        })
    }

    /// Obtain the number of primary and logical partitions, in that order.
    fn get_partition_type_count(&self) -> (usize, usize) {
        self.partitions
            .iter()
            .fold((0, 0), |sum, part| match part.part_type {
                PartitionType::Logical => (sum.0, sum.1 + 1),
                PartitionType::Primary => (sum.0 + 1, sum.1),
            })
    }

    /// Unmounts all partitions on the device
    pub fn unmount_all_partitions(&mut self) -> Result<(), io::Error> {
        info!(
            "libdistinst: unmount all partitions on {}",
            self.path().display()
        );

        for partition in &mut self.partitions {
            if let Some(ref mount) = partition.mount_point {
                info!(
                    "libdistinst: unmounting {}, which is mounted at {}",
                    partition.get_device_path().display(),
                    mount.display()
                );

                umount(mount, false)?;
            }

            partition.mount_point = None;

            if partition.swapped {
                info!(
                    "libdistinst: unswapping '{}'",
                    partition.get_device_path().display(),
                );
                swapoff(&partition.get_device_path())?;
            }

            partition.swapped = false;
        }

        Ok(())
    }

    /// Drops all partitions in the in-memory disk representation, and marks that a new
    /// partition table should be written to the disk during the disk operations phase.
    pub fn mklabel(&mut self, kind: PartitionTable) -> Result<(), DiskError> {
        info!(
            "libdistinst: specifying to write new table on {}",
            self.path().display()
        );
        self.unmount_all_partitions()
            .map_err(|why| DiskError::Unmount { why })?;

        self.partitions.clear();
        self.mklabel = true;
        self.table_type = Some(kind);
        Ok(())
    }

    /// Marks that the partition should be removed.
    ///
    /// Partitions marked as source partitions (pre-existing on disk) will have their `remove`
    /// field set to `true`, whereas all other theoretical partitions will simply be removed
    /// from the partition vector.
    pub fn remove_partition(&mut self, partition: i32) -> Result<(), DiskError> {
        info!(
            "libdistinst: specifying to remove partition {} on {}",
            partition,
            self.path().display()
        );
        let id = self.partitions
            .iter_mut()
            .enumerate()
            .find(|&(_, ref p)| p.number == partition)
            .ok_or(DiskError::PartitionNotFound { partition })
            .map(|(id, p)| {
                if p.is_source {
                    p.remove = true;
                    0
                } else {
                    id
                }
            })?;

        if id != 0 {
            self.partitions.remove(id);
        }

        Ok(())
    }

    /// Returns the device type information as a string.
    pub fn get_device_type(&self) -> &str { &self.device_type }

    /// Obtains an immutable reference to a partition within the partition scheme.
    pub fn get_partition(&self, partition: i32) -> Option<&PartitionInfo> {
        self.partitions.iter().find(|part| part.number == partition)
    }

    /// Obtains a mutable reference to a partition within the partition scheme.
    pub fn get_partition_mut(&mut self, partition: i32) -> Option<&mut PartitionInfo> {
        self.partitions
            .iter_mut()
            .find(|part| part.number == partition)
    }

    /// Designates that the provided partition number should be resized so that the end sector
    /// will be located at the provided `end` value, and checks whether or not that this will
    /// be possible to do.
    pub fn resize_partition(&mut self, partition: i32, end: u64) -> Result<(), DiskError> {
        let end = end - 1;
        info!(
            "libdistinst: specifying to resize partition {} on {} to sector {}",
            partition,
            self.path().display(),
            end
        );

        let sector_size = self.sector_size;
        let (backup, num, start);
        {
            let partition = self.get_partition_mut(partition)
                .ok_or(DiskError::PartitionNotFound { partition })?;

            if end < partition.start_sector
                || end - partition.start_sector <= (10 * 1024 * 1024) / sector_size
            {
                return Err(DiskError::ResizeTooSmall);
            }

            backup = partition.end_sector;
            num = partition.number;
            start = partition.start_sector;
            partition.end_sector = end;
        }

        // Ensure that the new dimensions are not overlapping.
        if let Some(id) = self.overlaps_region_excluding(start, end, num) {
            let partition = self.get_partition_mut(partition).unwrap();
            partition.end_sector = backup;
            return Err(DiskError::SectorOverlaps { id });
        }

        Ok(())
    }

    /// Designates that the provided partition number should be moved to a specified sector,
    /// and calculates whether it will be possible to do that.
    pub fn move_partition(&mut self, partition: i32, start: u64) -> Result<(), DiskError> {
        info!(
            "libdistinst: specifying to move partition {} on {} to sector {}",
            partition,
            self.path().display(),
            start
        );
        let end = {
            let partition = self.get_partition_mut(partition)
                .ok_or(DiskError::PartitionNotFound { partition })?;

            if start == partition.start_sector {
                return Ok(());
            }

            if start > partition.start_sector {
                partition.end_sector + (start - partition.start_sector)
            } else {
                partition.end_sector - (partition.start_sector - start)
            }
        };

        if let Some(id) = self.overlaps_region_excluding(start, end, partition) {
            return Err(DiskError::SectorOverlaps { id });
        }

        let partition = self.get_partition_mut(partition).unwrap();

        partition.start_sector = start;
        partition.end_sector = end;
        Ok(())
    }

    /// Designates that the specified partition ID should be formatted with the given file
    /// system.
    ///
    /// # Note
    ///
    /// The partition name will cleared after calling this function.
    pub fn format_partition(
        &mut self,
        partition: i32,
        fs: FileSystemType,
    ) -> Result<(), DiskError> {
        info!(
            "libdistinst: specifying to format partition {} on {} with {:?}",
            partition,
            self.path().display(),
            fs,
        );
        let sector_size = self.sector_size;
        self.get_partition_mut(partition)
            .ok_or(DiskError::PartitionNotFound { partition })
            .and_then(|partition| {
                check_partition_size(partition.sectors() * sector_size, fs.clone())
                    .map_err(DiskError::from)
                    .map(|_| {
                        partition.format_with(fs);
                        ()
                    })
            })
    }

    /// Rewrites the partition flags on the given partition with the specified flags.
    pub fn add_flags(
        &mut self,
        partition: i32,
        flags: Vec<PartitionFlag>,
    ) -> Result<(), DiskError> {
        self.get_partition_mut(partition)
            .ok_or(DiskError::PartitionNotFound { partition })
            .map(|partition| {
                partition.flags = flags;
                ()
            })
    }

    /// Specifies to set a new label on the partition.
    pub fn set_name(&mut self, partition: i32, name: String) -> Result<(), DiskError> {
        self.get_partition_mut(partition)
            .ok_or(DiskError::PartitionNotFound { partition })
            .map(|partition| {
                partition.name = Some(name);
                ()
            })
    }

    /// Returns a partition ID if the given sector is within that partition.
    fn get_partition_at(&self, sector: u64) -> Option<i32> {
        self.partitions.iter()
            // Only consider partitions which are not set to be removed.
            .filter(|part| !part.remove)
            // Return upon the first partition where the sector is within the partition.
            .find(|part| sector >= part.start_sector && sector <= part.end_sector)
            // If found, return the partition number.
            .map(|part| part.number)
    }

    /// If a given start and end range overlaps a pre-existing partition, that
    /// partition's number will be returned to indicate a potential conflict.
    ///
    /// Allows for a partition to be excluded from the search.
    fn overlaps_region_excluding(&self, start: u64, end: u64, exclude: i32) -> Option<i32> {
        self.partitions.iter()
            // Only consider partitions which are not set to be removed,
            // and are not to be excluded.
            .filter(|part| !part.remove && part.number != exclude)
            // Return upon the first partition where the sector is within the partition.
            .find(|part|
                !(
                    (start < part.start_sector && end < part.start_sector)
                    || (start > part.end_sector && end > part.end_sector)
                )
            )
            // If found, return the partition number.
            .map(|part| part.number)
    }

    /// Returns an error if the new disk does not contain the same source partitions.
    fn validate_layout(&self, new: &Disk) -> Result<(), DiskError> {
        if !new.mklabel {
            let mut new_parts = new.partitions.iter();
            for source in &self.partitions {
                match new_parts.next() {
                    Some(new) => if !source.is_same_partition_as(new) {
                        return Err(DiskError::LayoutChanged);
                    },
                    None => return Err(DiskError::LayoutChanged),
                }
            }
        }

        Ok(())
    }

    /// Compares the source disk's partition scheme to a possible new partition scheme.
    ///
    /// An error can occur if the layout of the new disk conflicts with the source.
    fn diff<'a>(&'a self, new: &Disk) -> Result<DiskOps<'a>, DiskError> {
        info!(
            "libdistinst: generating diff of disk at {}",
            self.path().display()
        );
        self.validate_layout(new)?;

        /// This function is only safe to use within the diff method. The purpose of
        /// this function is to sort the source partitions within the source and new
        /// partitions so that operations are committed in the correct order.
        fn sort_partitions<'a>(
            source: &'a [PartitionInfo],
            new: &'a [PartitionInfo],
        ) -> (Vec<&'a PartitionInfo>, Vec<&'a PartitionInfo>) {
            let mut new_sorted: Vec<&PartitionInfo> = Vec::new();
            let mut old_sorted: Vec<&PartitionInfo> = Vec::new();

            let mut partition_iter = new.iter();
            let mut old_iter = source.iter();

            while let Some(partition) = partition_iter.next() {
                if let Some(old_part) = old_iter.next() {
                    if partition.number != -1 {
                        if let Some(old_part) = source
                            .iter()
                            .find(|part| part.number == partition.number + 1)
                        {
                            if old_part.number != -1 && partition.end_sector > old_part.start_sector
                            {
                                new_sorted.push(partition_iter.next().unwrap());
                                old_sorted.push(old_iter.next().unwrap());
                            }
                        }
                    }
                    old_sorted.push(old_part);
                }
                new_sorted.push(partition);
            }

            // Ensure that the new vectors are the same size as the unsorted ones.
            debug_assert!(new_sorted.len() == new.len() && old_sorted.len() == source.len());

            (new_sorted, old_sorted)
        }

        let mut remove_partitions = Vec::new();
        let mut change_partitions = Vec::new();
        let mut create_partitions = Vec::new();

        let sector_size = new.sector_size;
        let device_path = new.device_path.clone();

        let (new_sorted, old_sorted): (Vec<&PartitionInfo>, Vec<&PartitionInfo>) = if !new.mklabel {
            sort_partitions(&self.partitions, &new.partitions)
        } else {
            (new.partitions.iter().collect(), Vec::new())
        };

        info!("libdistinst: proposed layout:{}", {
            let mut output = String::new();
            for partition in &new_sorted {
                output.push_str(&format!(
                    "\n\t{}: {} - {}",
                    partition.number, partition.start_sector, partition.end_sector
                ));
            }
            output
        });

        let mut new_parts = new_sorted.iter();
        let mut new_part = None;

        fn flags_diff<I: Iterator<Item = PartitionFlag>>(
            source: &[PartitionFlag],
            flags: I,
        ) -> Vec<PartitionFlag> {
            flags.filter(|f| !source.contains(f)).collect()
        }

        let mklabel = if new.mklabel {
            new.table_type
        } else {
            'outer: for source in &old_sorted {
                loop {
                    let next_part = new_part.take().or_else(|| new_parts.next());
                    if let Some(new) = next_part {
                        // Source partitions may be removed or changed.
                        if new.is_source {
                            if source.number != new.number {
                                unreachable!(
                                    "layout validation: wrong number: {} != {}",
                                    new.number, source.number
                                );
                            }

                            if new.remove {
                                remove_partitions.push(new.number);
                                continue 'outer;
                            }

                            if source.requires_changes(new) {
                                if new.format || source.filesystem == Some(FileSystemType::Swap) {
                                    remove_partitions.push(new.number);
                                    create_partitions.push(PartitionCreate {
                                        path:         self.device_path.clone(),
                                        start_sector: new.start_sector,
                                        end_sector:   new.end_sector,
                                        format:       true,
                                        file_system:  Some(new.filesystem.clone().unwrap()),
                                        kind:         new.part_type,
                                        flags:        new.flags.clone(),
                                        label:        new.name.clone(),
                                    });
                                } else {
                                    change_partitions.push(PartitionChange {
                                        device_path: device_path.clone(),
                                        path: new.device_path.clone(),
                                        num: source.number,
                                        kind: new.part_type,
                                        start: new.start_sector,
                                        end: new.end_sector,
                                        sector_size,
                                        filesystem: source.filesystem.clone(),
                                        flags: flags_diff(
                                            &source.flags,
                                            new.flags.clone().into_iter(),
                                        ),
                                        new_flags: new.flags.clone(),
                                        label: new.name.clone(),
                                    });
                                }
                            }

                            continue 'outer;
                        } else {
                            // Non-source partitions should not be discovered at this stage.
                            unreachable!("layout validation: less sources");
                        }
                    }
                }
            }

            None
        };

        // Handle all of the non-source partitions, which are to be added to the disk.
        for partition in new_parts {
            if partition.is_source {
                unreachable!("layout validation: extra sources")
            }

            create_partitions.push(PartitionCreate {
                path:         self.device_path.clone(),
                start_sector: partition.start_sector,
                end_sector:   partition.end_sector,
                format:       true,
                file_system:  Some(partition.filesystem.clone().unwrap()),
                kind:         partition.part_type,
                flags:        partition.flags.clone(),
                label:        partition.name.clone(),
            });
        }

        Ok(DiskOps {
            mklabel,
            device_path: &self.device_path,
            remove_partitions,
            change_partitions,
            create_partitions,
        })
    }

    /// Attempts to commit all changes that have been made to the disk.
    pub fn commit(&mut self) -> Result<(), DiskError> {
        info!(
            "libdistinst: committing changes to {}",
            self.path().display()
        );
        Disk::from_name_with_serial(&self.device_path, &self.serial).and_then(|source| {
            source.diff(self).and_then(|ops| {
                ops.remove()
                    .and_then(|ops| ops.change())
                    .and_then(|ops| ops.create())
                    .and_then(|ops| ops.format())
            })
        })?;

        self.reload()
    }

    /// Reloads the disk information from the disk into our in-memory representation.
    pub fn reload(&mut self) -> Result<(), DiskError> {
        info!(
            "libdistinst: reloading disk information for {}",
            self.path().display()
        );

        // TODO: Collect less

        // Ensure that mount targets are carried over in the new data.
        let mounts: Vec<(u64, PathBuf)> = self.partitions
            .iter()
            .filter_map(|p| match p.target {
                Some(ref path) => Some((p.start_sector, path.to_path_buf())),
                None => None,
            })
            .collect();

        // Ensure that volume groups are carried over in the new data.
        let vol_groups: Vec<(u64, (String, Option<LvmEncryption>))> = self.partitions
            .iter()
            .filter_map(|p| {
                p.volume_group
                    .as_ref()
                    .map(|vg| (p.start_sector, vg.clone()))
            })
            .collect();

        // Same for key IDs
        let key_ids: Vec<(u64, (String, PathBuf))> = self.partitions
            .iter()
            .filter_map(|p| p.key_id.as_ref().map(|id| (p.start_sector, id.clone())))
            .collect();

        *self = Disk::from_name_with_serial(&self.device_path, &self.serial)?;

        for (sector, mount) in mounts {
            info!("libdistinst: checking for mount target at {}", sector);
            let part = self.get_partition_at(sector)
                .and_then(|num| self.get_partition_mut(num))
                .expect("partition sectors are off");
            part.target = Some(mount);
        }

        for (sector, volgroup) in vol_groups {
            info!("libdistinst: checking for mount target at {}", sector);
            let part = self.get_partition_at(sector)
                .and_then(|num| self.get_partition_mut(num))
                .expect("partition sectors are off");
            part.volume_group = Some(volgroup);
        }

        for (sector, id) in key_ids {
            info!("libdistinst: checking for mount target at {}", sector);
            let part = self.get_partition_at(sector)
                .and_then(|num| self.get_partition_mut(num))
                .expect("partition sectors are off");
            part.key_id = Some(id);
        }

        Ok(())
    }

    pub fn path(&self) -> &Path { &self.device_path }
}

/// A configuration of disks, both physical and logical.
pub struct Disks {
    physical: Vec<Disk>,
    logical:  Vec<LvmDevice>,
}

impl Disks {
    pub fn new() -> Disks {
        Disks {
            physical: Vec::new(),
            logical:  Vec::new(),
        }
    }

    /// Adds a disk to the disks configuration.
    pub fn add(&mut self, disk: Disk) { self.physical.push(disk); }

    /// Adds a logical disk (`LvmDevice`) to the list of disks.
    pub fn add_logical(&mut self, device: LvmDevice) { self.logical.push(device); }

    /// Returns a slice of physical disks stored within the configuration.
    pub fn get_physical_devices(&self) -> &[Disk] { &self.physical }

    /// Returns a mutable slice of physical disks stored within the configuration.
    pub fn get_physical_devices_mut(&mut self) -> &mut [Disk] { &mut self.physical }

    /// Returns a slice of logical disks stored within the configuration.
    pub fn get_logical_devices(&self) -> &[LvmDevice] { &self.logical }

    /// Returns a mutable slice of logical disks stored within the configuration.
    pub fn get_logical_devices_mut(&mut self) -> &mut [LvmDevice] { &mut self.logical }

    /// Returns a list of device paths which will be modified by this configuration.
    pub fn get_device_paths_to_modify(&self) -> Vec<PathBuf> {
        let mut output = Vec::new();
        for dev in self.get_physical_devices() {
            if dev.mklabel {
                // Devices with this set no longer hold the original source partitions.
                // TODO: Maybe have a backup field with the old partitions?
                let disk = Disk::from_name_with_serial(&dev.device_path, &dev.serial).unwrap();
                for part in disk.get_partitions()
                    .iter()
                    .map(|part| part.get_device_path())
                {
                    output.push(part.to_path_buf());
                }
            } else {
                for part in dev.get_partitions()
                    .iter()
                    .filter(|part| part.is_source && (part.remove || part.format))
                    .map(|part| part.get_device_path())
                {
                    output.push(part.to_path_buf());
                }
            }
        }

        output
    }

    /// Deactivates all device maps associated with the inner disks/partitions to be modified.
    pub fn deactivate_device_maps(&self) -> Result<(), DiskError> {
        let mounts = Mounts::new().unwrap();
        let umount = move |vg: &str| -> Result<(), DiskError> {
            for lv in lvs(vg).map_err(|why| DiskError::ExternalCommand { why })? {
                if let Some(mount) = mounts.get_mount_point(&lv) {
                    info!(
                        "libdistinst: unmounting logical volume mounted at {}",
                        mount.display()
                    );
                    umount(&mount, false).map_err(|why| DiskError::Unmount { why })?;
                }
            }

            Ok(())
        };

        let devices_to_modify = self.get_device_paths_to_modify();
        info!("libdistinst: devices to modify: {:?}", devices_to_modify);
        let volume_map = pvs().map_err(|why| DiskError::ExternalCommand { why })?;
        info!("libdistinst: volume map: {:?}", volume_map);
        let pvs = lvm::physical_volumes_to_deactivate(&devices_to_modify);
        info!("libdistinst: pvs: {:?}", pvs);

        // Handle LVM on LUKS
        for pv in &pvs {
            match volume_map.get(pv) {
                Some(&Some(ref vg)) => umount(vg).and_then(|_| {
                    deactivate_volumes(vg)
                        .and_then(|_| vgremove(vg))
                        .and_then(|_| pvremove(pv))
                        .and_then(|_| cryptsetup_close(pv))
                        .map_err(|why| DiskError::ExternalCommand { why })
                })?,
                Some(&None) => {
                    cryptsetup_close(pv).map_err(|why| DiskError::ExternalCommand { why })?
                }
                None => (),
            }
        }

        // Handle LVM without LUKS
        for entry in devices_to_modify
            .iter()
            .filter_map(|dev| volume_map.get(dev))
            .unique()
        {
            if let Some(ref vg) = *entry {
                vgremove(vg).map_err(|why| DiskError::ExternalCommand { why })?;
            }
        }

        Ok(())
    }

    /// Probes for and returns disk information for every disk in the system.
    pub fn probe_devices() -> Result<Disks, DiskError> {
        let mut disks = Disks::new();
        for mut device in Device::devices(true) {
            match device.type_() {
                DeviceType::PED_DEVICE_UNKNOWN
                | DeviceType::PED_DEVICE_LOOP
                | DeviceType::PED_DEVICE_FILE => continue,
                _ => disks.add(Disk::new(&mut device)?),
            }
        }

        // TODO: Also collect LVM devices
        Ok(disks)
    }

    /// Returns an immutable reference to the disk specified by its path, if it exists.
    pub fn find_disk<P: AsRef<Path>>(&self, path: P) -> Option<&Disk> {
        self.physical
            .iter()
            .find(|disk| &disk.device_path == path.as_ref())
    }

    /// Returns a mutable reference to the disk specified by its path, if it exists.
    pub fn find_disk_mut<P: AsRef<Path>>(&mut self, path: P) -> Option<&mut Disk> {
        self.physical
            .iter_mut()
            .find(|disk| &disk.device_path == path.as_ref())
    }

    /// Returns an immutable reference to the disk specified by its path, if it exists.
    pub fn find_logical_disk(&self, group: &str) -> Option<&LvmDevice> {
        self.logical
            .iter()
            .find(|device| &device.volume_group == group)
    }

    /// Returns a mutable reference to the disk specified by its path, if it exists.
    pub fn find_logical_disk_mut(&mut self, group: &str) -> Option<&mut LvmDevice> {
        self.logical
            .iter_mut()
            .find(|device| &device.volume_group == group)
    }

    /// Finds the partition block path and associated partition information that is associated with
    /// the given target mount point. Scans both physical and logical partitions.
    pub fn find_partition<'a>(&'a self, target: &Path) -> Option<(&'a Path, &'a PartitionInfo)> {
        find_partition(&self.physical, target).or(find_partition(&self.logical, target))
    }

    pub fn find_volume_paths<'a>(&'a self, volume_group: &str) -> Vec<(&'a Path, &'a Path)> {
        let mut volumes = Vec::new();

        for disk in &self.physical {
            for partition in disk.get_partitions() {
                if let Some(ref pvolume_group) = partition.volume_group {
                    if pvolume_group.0 == volume_group {
                        volumes.push((disk.get_device_path(), partition.get_device_path()));
                    }
                }
            }
        }

        volumes
    }

    /// Obtains the paths to the device and partition block paths where the root and EFI
    /// partitions are installed. The paths for the EFI partition will not be collected if
    /// the provided boot loader was of the EFI variety.
    pub fn get_base_partitions(
        &self,
        bootloader: Bootloader,
    ) -> ((&Path, &PartitionInfo), Option<(&Path, &PartitionInfo)>) {
        match bootloader {
            Bootloader::Bios => {
                let root = self.find_partition(Path::new("/")).expect(
                    "verify_partitions() should have ensured that a root partition was created",
                );

                (root, None)
            }
            Bootloader::Efi => {
                let efi = self.find_partition(Path::new("/boot/efi")).expect(
                    "verify_partitions() should have ensured that an EFI partition was created",
                );

                let root = self.find_partition(Path::new("/")).expect(
                    "verify_partitions() should have ensured that a root partition was created",
                );

                (root, Some(efi))
            }
        }
    }

    /// Ensure that keyfiles have key paths.
    pub fn verify_keyfile_paths(&self) -> Result<(), DiskError> {
        info!("libdistinst: verifying if keyfiles have paths");
        let mut set = HashSet::new();
        'outer: for logical_device in &self.logical {
            if let Some(ref encryption) = logical_device.encryption {
                if let Some((ref key_id, _)) = encryption.keydata {
                    // Ensure that the root partition is not on this encrypted device.
                    // The keyfile paths need to be mountable by an already-decrypted root.
                    for partition in logical_device.get_partitions() {
                        if Some(Path::new("/").into()) == partition.target {
                            return Err(DiskError::KeyContainsRoot);
                        }
                    }

                    let partitions = self.physical.iter().flat_map(|p| p.partitions.iter());
                    for partition in partitions {
                        if let Some((ref pkey_id, _)) = partition.key_id {
                            if pkey_id == key_id {
                                if set.contains(&key_id) {
                                    return Err(DiskError::KeyPathAlreadySet { id: key_id.clone() });
                                }
                                set.insert(key_id);
                                continue 'outer;
                            }
                        }
                    }
                    return Err(DiskError::KeyWithoutPath);
                }
            }
        }

        Ok(())
    }

    /// Maps key paths to their keyfile IDs
    fn resolve_keyfile_paths(&mut self) -> Result<(), DiskError> {
        let mut temp: Vec<(String, Option<(PathBuf, PathBuf)>)> = Vec::new();

        'outer: for logical_device in &mut self.logical {
            if let Some(ref mut encryption) = logical_device.encryption {
                if let Some((ref key_id, ref mut paths)) = encryption.keydata {
                    let partitions = self.physical.iter().flat_map(|p| p.partitions.iter());
                    for partition in partitions {
                        let dev = partition.get_device_path();
                        if let Some((ref pkey_id, ref pkey_mount)) = partition.key_id {
                            if pkey_id == key_id {
                                *paths = Some((dev.into(), pkey_mount.into()));
                                temp.push((pkey_id.clone(), paths.clone()));
                                continue 'outer;
                            }
                        }
                    }
                    return Err(DiskError::KeyWithoutPath);
                }
            }
        }

        for (key, paths) in temp {
            let partitions = self.physical
                .iter_mut()
                .flat_map(|x| x.get_partitions_mut().iter_mut())
                .chain(
                    self.logical
                        .iter_mut()
                        .flat_map(|x| x.get_partitions_mut().iter_mut()),
                );

            for partition in partitions {
                if let Some(&mut (_, Some(ref mut enc))) = partition.volume_group.as_mut() {
                    if let Some((ref id, ref mut ppath)) = enc.keydata {
                        if &*id == &*key {
                            *ppath = paths.clone();
                            continue;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Ensures that EFI installs contain a `/boot/efi` and `/` partition, whereas MBR installs
    /// contain a `/` partition. Additionally, the EFI partition must have the ESP flag set.
    pub fn verify_partitions(&self, bootloader: Bootloader) -> io::Result<()> {
        let (_, root) = self.find_partition(Path::new("/")).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "root partition was not defined",
            )
        })?;

        use FileSystemType::*;
        match root.filesystem {
            Some(Fat16) | Some(Fat32) | Some(Ntfs) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "root partition has invalid file system",
                ));
            }
            Some(_) => (),
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "root partition does not have a file system",
                ));
            }
        }

        if bootloader == Bootloader::Efi {
            let (_, efi) = self.find_partition(Path::new("/boot/efi")).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "EFI partition was not defined")
            })?;

            if !efi.flags.contains(&PartitionFlag::PED_PARTITION_ESP) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "EFI partition did not have ESP flag set",
                ));
            }

            match efi.filesystem {
                Some(Fat16) | Some(Fat32) => (),
                Some(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "efi partition has invalid file system",
                    ));
                }
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "efi partition does not have a file system",
                    ));
                }
            }
        }

        // TODO:

        Ok(())
    }

    /// Generates fstab entries in memory
    pub(crate) fn generate_fstab(&self) -> OsString {
        info!("libdistinst: generating fstab in memory");
        let mut fstab = OsString::with_capacity(1024);

        let fs_entries = self.physical
            .iter()
            .flat_map(|disk| disk.partitions.iter())
            .filter_map(|part| part.get_block_info());

        let logical_entries = self.logical
            .iter()
            .flat_map(|disk| disk.partitions.iter())
            .filter_map(|part| part.get_block_info());

        // <file system>  <mount point>  <type>  <options>  <dump>  <pass>
        for entry in fs_entries.chain(logical_entries) {
            fstab.reserve_exact(entry.len() + 16);
            fstab.push("UUID=");
            fstab.push(&entry.uuid);
            fstab.push("  ");
            fstab.push(entry.mount());
            fstab.push("  ");
            fstab.push(&entry.fs);
            fstab.push("  ");
            fstab.push(&entry.options);
            fstab.push("  ");
            fstab.push(if entry.dump { "1" } else { "0" });
            fstab.push("  ");
            fstab.push(if entry.pass { "1" } else { "0" });
            fstab.push("\n");
        }

        info!(
            "libdistinst: generated the following fstab data:\n{}\n",
            fstab.to_string_lossy(),
        );

        fstab.shrink_to_fit();
        fstab
    }

    /// Similar to `generate_fstab`, but for the crypttab file.
    pub(crate) fn generate_crypttab(&self) -> OsString {
        info!("libdistinst: generating crypttab in memory");
        let mut crypttab = OsString::with_capacity(1024);

        let partitions = self.physical
            .iter()
            .flat_map(|x| x.get_partitions().iter())
            .chain(self.logical.iter().flat_map(|x| x.get_partitions().iter()));

        // <PV> <UUID> <Pass> <Options>
        use std::borrow::Cow;
        use std::ffi::OsStr;
        for partition in partitions {
            if let Some(&(_, Some(ref enc))) = partition.volume_group.as_ref() {
                let password: Cow<'static, OsStr> =
                    match (enc.password.is_some(), enc.keydata.as_ref()) {
                        (true, None) => Cow::Borrowed(OsStr::new("none")),
                        (false, None) => Cow::Borrowed(OsStr::new("/dev/urandom")),
                        (true, Some(key)) => unimplemented!(),
                        (false, Some(&(_, ref key))) => {
                            let path = key.clone()
                                .expect("should have been populated")
                                .1
                                .join(&enc.physical_volume);
                            Cow::Owned(path.into_os_string())
                        }
                    };

                match get_uuid(&partition.device_path) {
                    Some(uuid) => {
                        crypttab.push(&enc.physical_volume);
                        crypttab.push(" UUID=");
                        crypttab.push(&uuid);
                        crypttab.push(" ");
                        crypttab.push(&password);
                        crypttab.push(" luks\n");
                    }
                    None => error!(
                        "unable to find UUID for {} -- skipping",
                        partition.device_path.display()
                    ),
                }
            }
        }

        info!(
            "libdistinst: generated the following crypttab data:\n{}\n",
            crypttab.to_string_lossy(),
        );

        crypttab.shrink_to_fit();
        crypttab
    }

    /// Generates intial LVM devices with a clean slate, using partition information.
    /// TODO: This should consume `Disks` and return a locked state.
    pub fn initialize_volume_groups(&mut self) -> io::Result<()> {
        let logical = &mut self.logical;
        let physical = &self.physical;
        logical.clear();

        for disk in physical {
            let sector_size = disk.get_sector_size();
            for partition in disk.get_partitions() {
                if let Some(ref lvm) = partition.volume_group {
                    // TODO: NLL
                    let push = match logical.iter_mut().find(|d| &d.volume_group == &lvm.0) {
                        Some(device) => {
                            device.add_sectors(partition.sectors());
                            false
                        }
                        None => true,
                    };

                    if push {
                        logical.push(LvmDevice::new(
                            lvm.0.clone(),
                            lvm.1.clone(),
                            partition.sectors(),
                            sector_size,
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Applies all logical device operations, which are to be performed after all physical disk
    /// operations have completed.
    pub(crate) fn commit_logical_partitions(&mut self) -> Result<(), DiskError> {
        // First we verify that we have a valid logical layout.
        for device in &self.logical {
            let volumes = self.find_volume_paths(&device.volume_group);
            debug_assert!(!volumes.is_empty());
            if device.encryption.is_some() && volumes.len() > 1 {
                return Err(DiskError::SameGroup);
            }
            device.validate()?;
        }

        // By default, the `device_path` field is not populated, so let's fix that.
        for device in &mut self.logical {
            for partition in &mut device.partitions {
                let label = partition.name.as_ref().unwrap();
                partition.device_path =
                    PathBuf::from(format!("/dev/mapper/{}-{}", device.volume_group, label));
            }
        }

        // Ensure that the keyfile paths are mapped to their mount targets.
        self.resolve_keyfile_paths()?;

        // Now we will apply the logical layout.
        for device in &self.logical {
            let volumes: Vec<(&Path, &Path)> = self.find_volume_paths(&device.volume_group);
            let mut device_path = None;

            if let Some(encryption) = device.encryption.as_ref() {
                encryption.encrypt(volumes[0].1)?;
                encryption.open(volumes[0].1)?;
                encryption.create_physical_volume()?;
                device_path = Some(PathBuf::from(
                    ["/dev/mapper/", &encryption.physical_volume].concat(),
                ));
            }

            // Obtains an iterator which may produce one or more device paths.
            let volumes: Box<Iterator<Item = &Path>> = match device_path.as_ref() {
                // There will be only one volume, which we obtained from encryption.
                Some(path) => Box::new(iter::once(path.as_path())),
                // There may be more than one volume within a unencrypted LVM config.
                None => Box::new(volumes.into_iter().map(|(_, part)| part)),
            };

            device.create_volume_group(volumes)?;
            device.create_partitions()?;
        }

        Ok(())
    }
}

impl IntoIterator for Disks {
    type Item = Disk;
    type IntoIter = ::std::vec::IntoIter<Disk>;

    fn into_iter(self) -> Self::IntoIter { self.physical.into_iter() }
}

impl FromIterator<Disk> for Disks {
    fn from_iter<I: IntoIterator<Item = Disk>>(iter: I) -> Self {
        // TODO: Also collect LVM Devices
        Disks {
            physical: iter.into_iter().collect(),
            logical:  Vec::new(),
        }
    }
}

/// Obtains the UUID of the given device path by resolving symlinks in `/dev/disk/by-uuid`
/// until the device is found.
fn get_uuid(path: &Path) -> Option<OsString> {
    let uuid_dir = read_dir("/dev/disk/by-uuid").expect("unable to find /dev/disk/by-uuid");

    if let Ok(path) = path.canonicalize() {
        for uuid_entry in uuid_dir.filter_map(|entry| entry.ok()) {
            if &uuid_entry.path().canonicalize().unwrap() == &path {
                return Some(uuid_entry.file_name());
            }
        }
    }

    None
}

// TODO: Fix these tests
//
// #[cfg(test)]
// mod tests {
//     use super::*;

//     fn get_default() -> Disks {
//         Disks {
//             physical: vec![
//                 Disk {
//                     mklabel:     false,
//                     model_name:  "Test Disk".into(),
//                     serial:      "Test Disk 123".into(),
//                     device_path: "/dev/sdz".into(),
//                     size:        1953525168,
//                     sector_size: 512,
//                     device_type: "TEST".into(),
//                     table_type:  Some(PartitionTable::Gpt),
//                     read_only:   false,
//                     partitions:  vec![
//                         PartitionInfo {
//                             active:       true,
//                             busy:         true,
//                             is_source:    true,
//                             remove:       false,
//                             format:       false,
//                             device_path:  Path::new("/dev/sdz1").to_path_buf(),
//                             flags:        vec![],
//                             mount_point:  Some(Path::new("/boot/efi").to_path_buf()),
//                             target:       Some(Path::new("/boot/efi").to_path_buf()),
//                             start_sector: 2048,
//                             end_sector:   1026047,
//                             filesystem:   Some(FileSystemType::Fat16),
//                             name:         None,
//                             number:       1,
//                             part_type:    PartitionType::Primary,
//                             swapped:      false,
//                             key_id:       None,
//                             volume_group: None,
//                         },
//                         PartitionInfo {
//                             active:       true,
//                             busy:         true,
//                             is_source:    true,
//                             remove:       false,
//                             format:       false,
//                             device_path:  Path::new("/dev/sdz2").to_path_buf(),
//                             flags:        vec![],
//                             mount_point:  Some(Path::new("/").to_path_buf()),
//                             target:       Some(Path::new("/").to_path_buf()),
//                             start_sector: 1026048,
//                             end_sector:   420456447,
//                             filesystem:   Some(FileSystemType::Btrfs),
//                             name:         Some("Pop!_OS".into()),
//                             number:       2,
//                             part_type:    PartitionType::Primary,
//                             swapped:      false,
//                             key_id:       None,
//                             volume_group: None,
//                         },
//                         PartitionInfo {
//                             active:       false,
//                             busy:         false,
//                             is_source:    true,
//                             remove:       false,
//                             format:       false,
//                             device_path:  Path::new("/dev/sdz3").to_path_buf(),
//                             flags:        vec![],
//                             mount_point:  None,
//                             target:       None,
//                             start_sector: 420456448,
//                             end_sector:   1936738303,
//                             filesystem:   Some(FileSystemType::Ext4),
//                             name:         Some("Solus OS".into()),
//                             number:       3,
//                             part_type:    PartitionType::Primary,
//                             swapped:      false,
//                             key_id:       None,
//                             volume_group: None,
//                         },
//                         PartitionInfo {
//                             active:       true,
//                             busy:         false,
//                             is_source:    true,
//                             remove:       false,
//                             format:       false,
//                             device_path:  Path::new("/dev/sdz4").to_path_buf(),
//                             flags:        vec![],
//                             mount_point:  None,
//                             target:       None,
//                             start_sector: 1936738304,
//                             end_sector:   1953523711,
//                             filesystem:   Some(FileSystemType::Swap),
//                             name:         None,
//                             number:       4,
//                             part_type:    PartitionType::Primary,
//                             swapped:      false,
//                             key_id:       None,
//                             volume_group: None,
//                         },
//                     ],
//                 },
//             ],
//             logical: Vec::new(),
//         }
//     }

//     fn get_empty() -> Disks {
//         Disks {
//             physical: vec![
//                 Disk {
//                     mklabel:     false,
//                     model_name:  "Test Disk".into(),
//                     serial:      "Test Disk 123".into(),
//                     device_path: "/dev/sdz".into(),
//                     size:        1953525168,
//                     sector_size: 512,
//                     device_type: "TEST".into(),
//                     table_type:  Some(PartitionTable::Gpt),
//                     read_only:   false,
//                     partitions:  Vec::new(),
//                 },
//             ],
//             logical: Vec::new()
//         }
//     }

//     const GIB20: u64 = 41943040;

//     // 500 MiB Fat16 partition.
//     fn boot_part(start: u64) -> PartitionBuilder {
//         PartitionBuilder::new(start, 1024_000 + start, FileSystemType::Fat16)
//     }

//     // 20 GiB Ext4 partition.
//     fn root_part(start: u64) -> PartitionBuilder {
//         PartitionBuilder::new(start, GIB20 + start, FileSystemType::Ext4)
//     }

//     #[test]
//     fn layout_diff() {
//         let source = get_default().0.into_iter().next().unwrap();
//         let mut new = source.clone();
//         new.remove_partition(1).unwrap();
//         new.remove_partition(2).unwrap();
//         new.format_partition(3, FileSystemType::Xfs).unwrap();
//         new.resize_partition(3, GIB20).unwrap();
//         new.remove_partition(4).unwrap();
//         new.add_partition(boot_part(2048)).unwrap();
//         new.add_partition(root_part(1026_048)).unwrap();
//         assert_eq!(
//             source.diff(&new).unwrap(),
//             DiskOps {

//                 device_path:       Path::new("/dev/sdz"),
//                 remove_partitions: vec![1, 2, 4],
//                 change_partitions: vec![
//                     PartitionChange {
//                         num:    3,
//                         start:  420456448,
//                         end:    420456448 + GIB20,
//                         format: Some(FileSystemType::Xfs),
//                         flags:  vec![],
//                     },
//                 ],
//                 create_partitions: vec![
//                     PartitionCreate {
//                         start_sector: 2048,
//                         end_sector:   1024_000 + 2047,
//                         file_system:  FileSystemType::Fat16,
//                         kind:         PartitionType::Primary,
//                         flags:        vec![],
//                     },
//                     PartitionCreate {
//                         start_sector: 1026_048,
//                         end_sector:   GIB20 + 1026_047,
//                         file_system:  FileSystemType::Ext4,
//                         kind:         PartitionType::Primary,
//                         flags:        vec![],
//                     },
//                 ],
//             }
//         )
//     }

//     #[test]
//     fn partition_add() {
//         // The default sample is maxed out, so any partition added should fail.
//         let mut source = get_default().0.into_iter().next().unwrap();
//         assert!(
//             source
//                 .add_partition(PartitionBuilder::new(2048, 2_000_000, FileSystemType::Ext4))
//                 .is_err()
//         );

//         // Failures should also occur if the end sector exceeds the size of
//         assert!(
//             source
//                 .add_partition(PartitionBuilder::new(
//                     2048,
//                     1953525169,
//                     FileSystemType::Ext4
//                 ))
//                 .is_err()
//         );

//         // An empty disk should succeed, on the other hand.
//         let mut source = get_empty().0.into_iter().next().unwrap();

//         // Create 500MiB Fat16 partition w/ 512 byte sectors.
//         source.add_partition(boot_part(2048)).unwrap();

//         // This should fail with an off by one error, due to the start
//         // sector being located within the previous partition.
//         assert!(source.add_partition(root_part(1026_047)).is_err());

//         // Create 20GiB Ext4 partition after that.
//         source.add_partition(root_part(1026_048)).unwrap();
//     }

//     #[test]
//     fn layout_validity() {
//         // This test ensures that invalid layouts will raise a flag. An invalid layout is
//         // a layout which is missing some of the original source partitions.
//         let source = get_default().0.into_iter().next().unwrap();
//         let mut duplicate = source.clone();
//         assert!(source.validate_layout(&duplicate).is_ok());

//         // This should fail, because a critical source partition was removed.
//         duplicate.partitions.remove(0);
//         assert!(source.validate_layout(&duplicate).is_err());

//         // An empty partition should always succeed.
//         let source = get_empty().0.into_iter().next().unwrap();
//         let mut duplicate = source.clone();
//         assert!(source.validate_layout(&duplicate).is_ok());
//         duplicate
//             .add_partition(PartitionBuilder::new(
//                 2048,
//                 1024_00 + 2048,
//                 FileSystemType::Fat16,
//             ))
//             .unwrap();
//         assert!(source.validate_layout(&duplicate).is_ok());
//     }
// }
