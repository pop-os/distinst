use super::{get_device, open_disk, Disks};
use super::super::{
    check_partition_size, DiskError, DiskExt, FileSystemType, PartitionFlag, PartitionInfo,
    PartitionTable, PartitionType,
};
use super::super::lvm::LvmEncryption;
use super::super::mount::{swapoff, umount};
use super::super::operations::*;
use super::super::serial::get_serial;
use libparted::{Device, DeviceType};

use std::io;
use std::path::{Path, PathBuf};
use std::str;

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
    pub(crate) fn new(device: &mut Device) -> Result<Disk, DiskError> {
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
    pub(crate) fn from_name_with_serial<P: AsRef<Path>>(
        name: P,
        serial: &str,
    ) -> Result<Disk, DiskError> {
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

    /// Obtains an immutable reference to a partition within the partition
    /// scheme.
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

    /// Rewrites the partition flags on the given partition with the specified
    /// flags.
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

    /// Returns an error if the new disk does not contain the same source
    /// partitions.
    pub(crate) fn validate_layout(&self, new: &Disk) -> Result<(), DiskError> {
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
    pub(crate) fn diff<'a>(&'a self, new: &Disk) -> Result<DiskOps<'a>, DiskError> {
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

    /// Reloads the disk information from the disk into our in-memory
    /// representation.
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
