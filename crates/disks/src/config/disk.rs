use super::{
    super::{
        serial::get_serial, BlockDeviceExt, DiskError, DiskExt, Disks, FileSystem, PartitionError,
        PartitionFlag, PartitionInfo, PartitionTable, PartitionType,
    },
    partitions::{FORMAT, REMOVE, SOURCE, SWAPPED},
    PVS,
};
use disk_types::{PartitionExt, PartitionTableExt, SectorExt};
use crate::external::{is_encrypted, pvs};
use libparted::{Device, DeviceType, Disk as PedDisk};
use operations::{
    parted::{get_device, open_disk},
    *,
};
use partition_identity::PartitionID;
use proc_mounts::{MOUNTS, SWAPS};
use rayon::prelude::*;
use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
    str,
};
use sys_mount::{unmount, UnmountFlags};

/// Detects a partition on the device, if it exists.
/// Useful for detecting if a LUKS device has a file system.
pub fn detect_fs_on_device(path: &Path) -> Option<PartitionInfo> {
    if let Ok(mut dev) = Device::get(path) {
        if let Ok(disk) = PedDisk::new(&mut dev) {
            if let Some(part) = disk.parts().next() {
                unsafe {
                    if PVS.is_none() {
                        PVS = Some(pvs().expect("do you have the `lvm2` package installed?"));
                    }
                }

                let mounts = MOUNTS.read().expect("failed to get mounts in Disk::new");
                let swaps = SWAPS.read().expect("failed to get swaps in Disk::new");

                match PartitionInfo::new_from_ped(&part) {
                    Ok(mut part) => {
                        if let Some(part) = part.as_mut() {
                            let device_path = &part.device_path;
                            let original_vg = unsafe {
                                PVS.as_ref()
                                    .unwrap()
                                    .get(device_path)
                                    .and_then(|vg| vg.as_ref().cloned())
                            };

                            if let Some(ref vg) = original_vg.as_ref() {
                                info!("partition belongs to volume group '{}'", vg);
                            }

                            if part.filesystem.is_none() {
                                part.filesystem = if is_encrypted(device_path) {
                                    Some(FileSystem::Luks)
                                } else if original_vg.is_some() {
                                    Some(FileSystem::Lvm)
                                } else {
                                    None
                                };
                            }

                            part.mount_point = mounts.0
                                .iter()
                                .filter(|mount| &mount.source == device_path)
                                .map(|m| m.dest.clone())
                                .collect();
                            part.bitflags |=
                                if swaps.get_swapped(device_path) { SWAPPED } else { 0 };
                            part.original_vg = original_vg;
                        }
                        return part;
                    }
                    Err(why) => {
                        info!("unable to get partition from device: {}", why);
                    }
                }
            }
        }
    }

    None
}

/// Contains all of the information relevant to a given device.
///
/// # Note
///
/// The `device_path` field may be used for identification of the device in the system.
#[derive(Clone, Debug, PartialEq)]
pub struct Disk {
    /// The model name of the device, assigned by the manufacturer.
    pub model_name:  String,
    /// A unique identifier to this disk.
    pub serial:      String,
    /// The location in the file system where the block device is located.
    pub device_path: PathBuf,
    /// Account for the possibility that the entire disk is a file system.
    pub file_system: Option<PartitionInfo>,
    /// Where the device is mounted, if mounted at all.
    pub mount_point: Vec<PathBuf>,
    /// The size of the disk in sectors.
    pub size:        u64,
    /// The type of the device, such as SCSI.
    pub device_type: String,
    /// The partition table may be either **MSDOS** or **GPT**.
    pub table_type:  Option<PartitionTable>,
    /// Whether the device is currently in a read-only state.
    pub read_only:   bool,
    /// Defines whether the device should be wiped or not. The `table_type`
    /// field will be used to determine which table to write to the disk.
    pub mklabel:     bool,
    /// The partitions that are stored on the device.
    pub partitions:  Vec<PartitionInfo>,
}

impl BlockDeviceExt for Disk {
    fn get_device_path(&self) -> &Path { &self.device_path }

    fn get_mount_point(&self) -> &[PathBuf] { self.mount_point.as_ref() }

    fn is_read_only(&self) -> bool { self.read_only }
}

impl SectorExt for Disk {
    fn get_sectors(&self) -> u64 {
        self.size
    }
}

impl PartitionTableExt for Disk {
    fn get_partition_table(&self) -> Option<PartitionTable> { self.table_type }

    fn get_partition_type_count(&self) -> (usize, usize, bool) {
        self.partitions.iter().fold((0, 0, false), |sum, part| match part.get_partition_type() {
            PartitionType::Logical => (sum.0, sum.1 + 1, sum.2),
            PartitionType::Primary => (sum.0 + 1, sum.1, sum.2),
            PartitionType::Extended => (sum.0, sum.1, true),
        })
    }
}

impl DiskExt for Disk {
    const LOGICAL: bool = false;

    fn get_file_system(&self) -> Option<&PartitionInfo> { self.file_system.as_ref() }

    fn get_file_system_mut(&mut self) -> Option<&mut PartitionInfo> { self.file_system.as_mut() }

    fn set_file_system(&mut self, fs: PartitionInfo) { self.file_system = Some(fs) }

    fn get_model(&self) -> &str { &self.model_name }

    fn get_partitions_mut(&mut self) -> &mut [PartitionInfo] { &mut self.partitions }

    fn get_partitions(&self) -> &[PartitionInfo] { &self.partitions }

    fn push_partition(&mut self, partition: PartitionInfo) { self.partitions.push(partition); }
}

impl Disk {
    pub fn new(device: &mut Device, extended_partition_info: bool) -> Result<Disk, DiskError> {
        info!("obtaining disk information from {}", device.path().display());
        let model_name = device.model().into();
        let device_path = device.path().to_owned();
        let serial = match device.type_() {
            // Encrypted devices do not have serials
            DeviceType::PED_DEVICE_DM | DeviceType::PED_DEVICE_LOOP => "".into(),
            _ => get_serial(&device_path).unwrap_or_else(|_| "".into()),
        };

        let size = device.length();
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

        let mounts = MOUNTS.read().expect("failed to get mounts in Disk::new");
        let swaps = SWAPS.read().expect("failed to get swaps in Disk::new");

        Ok(Disk {
            model_name,
            mount_point: mounts.0
                .iter()
                .filter(|mount| &mount.source == &device_path)
                .map(|m| m.dest.clone())
                .collect(),
            device_path,
            file_system: None,
            serial,
            size,
            device_type,
            read_only,
            table_type,
            mklabel: false,
            partitions: if table_type.is_some() {
                let mut partitions = Vec::new();
                for (ordering, part) in disk.parts().filter(|part| part.num() != -1).enumerate() {
                    let part_result = PartitionInfo::new_from_ped(&part)
                        .map_err(|why| DiskError::MountsObtain { why })?;
                    if let Some(mut part) = part_result {
                        part.ordering = ordering as i32;
                        partitions.push(part);
                    }
                }

                if extended_partition_info {
                    unsafe {
                        if PVS.is_none() {
                            PVS = Some(pvs().expect("do you have the `lvm2` package installed?"));
                        }
                    }

                    partitions.par_iter_mut().for_each(|part| {
                        part.collect_extended_information(&mounts, &swaps);
                    });
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
        get_device(name).map_err(Into::into).and_then(|mut device| Disk::new(&mut device, true))
    }

    /// Obtains the disk that corresponds to a given serial model.
    ///
    /// First attempts to check if the supplied name has the valid serial number (highly likely),
    /// then performs a full probe of all disks in the system to attempt to find the matching
    /// serial number, in the event that the user swapped hard drive positions.
    ///
    /// If no match is found, then `Err(DiskError::DeviceGet)` is returned.
    pub fn from_name_with_serial<P: AsRef<Path>>(name: P, serial: &str) -> Result<Disk, DiskError> {
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

    /// Returns the serial of the device, filled in by the manufacturer.
    pub fn get_serial(&self) -> &str { &self.serial }

    pub fn is_being_modified(&self) -> bool {
        self.partitions.iter().any(|x| {
            x.bitflags & REMOVE != 0
                || x.bitflags & FORMAT != 0
                || x.target.is_some()
                || x.lvm_vg.is_some()
                || !x.subvolumes.is_empty()
        })
    }

    /// Unmounts all partitions on the device
    pub fn unmount_all_partitions(&mut self) -> Result<(), (PathBuf, io::Error)> {
        info!("unmount all partitions on {}", self.path().display());

        let swaps = SWAPS.read().expect("failed to get swaps in unmount_all_partitions");
        for partition in &mut self.partitions {
            for mount in partition.mount_point.iter().rev() {
                if mount == Path::new("/cdrom") || mount == Path::new("/") {
                    continue;
                }

                info!(
                    "unmounting {}, which is mounted at {}",
                    partition.get_device_path().display(),
                    mount.display()
                );

                unmount(mount, UnmountFlags::empty())
                    .map_err(|why| (partition.get_device_path().to_path_buf(), why))?;
            }

            partition.deactivate_if_swap(&swaps)?;
        }

        Ok(())
    }

    /// Unmounts all partitions on the device with a target
    pub fn unmount_all_partitions_with_target(&mut self) -> Result<(), (PathBuf, io::Error)> {
        info!("unmount all partitions with a target on {}", self.path().display());

        let swaps =
            SWAPS.read().expect("failed to get swaps in unmount_all_partitions_with_target");
        let mountstab =
            MOUNTS.read().expect("failed to get mounts in unmount_all_partitions_with_target");

        for partition in &mut self.partitions {
            partition.deactivate_if_swap(&swaps)?;
        }

        let mut mounts = BTreeSet::new();
        for mount in dbg!(&mountstab).source_starts_with(self.path()) {
            debug!("checking {:?}", mount);
            if mount.dest == Path::new("/cdrom")
                || mount.dest == Path::new("/")
                || mount.dest == Path::new("/boot/efi")
            {
                continue;
            }

            info!(
                "marking {} to be unmounted, which is mounted at {}",
                mount.source.display(),
                mount.dest.display(),
            );

            mounts.insert(&mount.dest);
            for target in mountstab.destination_starts_with(&mount.dest) {
                mounts.insert(&target.dest);
            }
        }

        for mount in mounts.into_iter().rev() {
            info!("unmounting {}", mount.display());
            unmount(&mount, UnmountFlags::empty()).map_err(|why| (mount.to_path_buf(), why))?;
        }

        Ok(())
    }

    /// Drops all partitions in the in-memory disk representation, and marks that a new
    /// partition table should be written to the disk during the disk operations phase.
    pub fn mklabel(&mut self, kind: PartitionTable) -> Result<(), DiskError> {
        info!("specifying to write new table on {}", self.path().display());
        self.unmount_all_partitions()
            .map_err(|(device, why)| DiskError::Unmount { device, why })?;

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
        info!("specifying to remove partition {} on {}", partition, self.path().display());
        let id = self
            .partitions
            .iter_mut()
            .enumerate()
            .find(|&(_, ref p)| p.number == partition)
            .ok_or(DiskError::PartitionNotFound { partition })
            .map(|(id, p)| {
                if p.flag_is_enabled(SOURCE) {
                    p.bitflags |= REMOVE;
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

    pub fn get_esp_partitions_mut(&mut self) -> Vec<&mut PartitionInfo> {
        self.partitions.iter_mut().filter(|p| p.is_esp_partition()).collect()
    }

    /// Obtains an immutable reference to a partition within the partition
    /// scheme.
    pub fn get_partition(&self, partition: i32) -> Option<&PartitionInfo> {
        self.partitions.iter().find(|part| part.number == partition)
    }

    /// Obtains a mutable reference to a partition within the partition scheme.
    pub fn get_partition_mut(&mut self, partition: i32) -> Option<&mut PartitionInfo> {
        self.partitions.iter_mut().find(|part| part.number == partition)
    }

    /// Find a partition by an identifier.
    pub fn get_partition_by_identity(&self, id: &PartitionID) -> Option<&PartitionInfo> {
        self.partitions.iter().find(|part| part.identifiers.matches(id))
    }

    pub fn get_partition_by_identity_mut(
        &mut self,
        id: &PartitionID,
    ) -> Option<&mut PartitionInfo> {
        self.partitions.iter_mut().find(|part| part.identifiers.matches(id))
    }

    /// Designates that the provided partition number should be resized so that the end sector
    /// will be located at the provided `end` value, and checks whether or not that this will
    /// be possible to do.
    pub fn resize_partition(&mut self, partition: i32, mut end: u64) -> Result<u64, DiskError> {
        let (backup, num, start);
        {
            let partition = self
                .get_partition_mut(partition)
                .ok_or(DiskError::PartitionNotFound { partition })?;

            if end < partition.start_sector {
                return Err(DiskError::new_partition_error(
                    partition.device_path.clone(),
                    PartitionError::ResizeTooSmall,
                ));
            }

            {
                let length = end - partition.start_sector;
                end -= length % (2 * 1024);
            }

            info!(
                "specifying to resize {} to {} sectors",
                partition.get_device_path().display(),
                end - partition.start_sector
            );

            assert_eq!(0, (end - partition.start_sector) % (2 * 1024));

            if end < partition.start_sector
                || end - partition.start_sector <= (10 * 1024 * 1024) / 512
            {
                return Err(DiskError::new_partition_error(
                    partition.device_path.clone(),
                    PartitionError::ResizeTooSmall,
                ));
            }

            backup = partition.end_sector;
            num = partition.number;
            start = partition.start_sector;
            partition.end_sector = end;
        }

        // Ensure that the new dimensions are not overlapping.
        if let Some(id) = self.overlaps_region_excluding(start, end, num) {
            let partition = self
                .get_partition_mut(partition)
                .expect("unable to find partition that should exist");
            partition.end_sector = backup;
            return Err(DiskError::SectorOverlaps { id });
        }

        Ok(end)
    }

    /// Designates that the provided partition number should be moved to a specified sector,
    /// and calculates whether it will be possible to do that.
    pub fn move_partition(&mut self, partition: i32, start: u64) -> Result<(), DiskError> {
        info!(
            "specifying to move partition {} on {} to sector {}",
            partition,
            self.path().display(),
            start
        );
        let end = {
            let partition = self
                .get_partition_mut(partition)
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

        let partition =
            self.get_partition_mut(partition).expect("unable to find partition that should exist");

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
    pub fn format_partition(&mut self, partition: i32, fs: FileSystem) -> Result<(), DiskError> {
        info!(
            "specifying to format partition {} on {} with {:?}",
            partition,
            self.path().display(),
            fs,
        );
        let sector_size = 512;
        self.get_partition_mut(partition)
            .ok_or(DiskError::PartitionNotFound { partition })
            .and_then(|partition| {
                fs.validate_size(partition.get_sectors() * sector_size)
                    .map_err(|why| {
                        DiskError::new_partition_error(partition.device_path.clone(), why)
                    })
                    .map(|_| {
                        partition.format_with(fs);
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
        self.get_partition_mut(partition).ok_or(DiskError::PartitionNotFound { partition }).map(
            |partition| {
                partition.flags = flags;
            },
        )
    }

    /// Specifies to set a new label on the partition.
    pub fn set_name(&mut self, partition: i32, name: String) -> Result<(), DiskError> {
        self.get_partition_mut(partition).ok_or(DiskError::PartitionNotFound { partition }).map(
            |partition| {
                partition.name = Some(name);
            },
        )
    }

    /// Returns a partition ID if the given sector is within that partition.
    fn get_partition_at(&self, sector: u64) -> Option<i32> {
        self.partitions
            .iter()
            // Only consider partitions which are not set to be removed.
            .filter(|part| !part.flag_is_enabled(REMOVE))
            // Return upon the first partition where the sector is within the partition.
            .find(|part| part.sector_lies_within(sector))
            // If found, return the partition number.
            .map(|part| part.number)
    }

    /// If a given start and end range overlaps a pre-existing partition, that
    /// partition's number will be returned to indicate a potential conflict.
    ///
    /// Allows for a partition to be excluded from the search.
    fn overlaps_region_excluding(&self, start: u64, end: u64, exclude: i32) -> Option<i32> {
        self.partitions
            .iter()
            // Only consider partitions which are not set to be removed,
            // and are not to be excluded.
            .filter(|part| !part.flag_is_enabled(REMOVE) && part.number != exclude)
            // Return upon the first partition where the sector is within the partition.
            .find(|part| part.sectors_overlap(start, end))
            // If found, return the partition number.
            .map(|part| part.number)
    }

    /// Returns an error if the new disk does not contain the same source
    /// partitions.
    pub fn validate_layout(&self, new: &Disk) -> Result<(), DiskError> {
        if !new.mklabel {
            let mut new_parts = new.partitions.iter();
            for source in &self.partitions {
                match new_parts.next() {
                    Some(new) => {
                        if !source.is_same_partition_as(new) {
                            return Err(DiskError::LayoutChanged);
                        }
                    }
                    None => return Err(DiskError::LayoutChanged),
                }
            }
        }

        Ok(())
    }

    /// Compares the source disk's partition scheme to a possible new partition scheme.
    ///
    /// An error can occur if the layout of the new disk conflicts with the source.
    pub fn diff<'a>(&'a self, new: &Disk) -> Result<DiskOps<'a>, DiskError> {
        info!("generating diff of disk at {}", self.path().display());
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
                    if partition.ordering != -1 {
                        if let Some(old_part) =
                            source.iter().find(|part| part.ordering == partition.ordering + 1)
                        {
                            if old_part.ordering != -1
                                && partition.end_sector > old_part.start_sector
                            {
                                new_sorted.push(
                                    partition_iter.next().expect("new partition was expected"),
                                );
                                old_sorted
                                    .push(old_iter.next().expect("old partition was expected"));
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

        let device_path = new.device_path.clone();

        let (new_sorted, old_sorted): (Vec<&PartitionInfo>, Vec<&PartitionInfo>) = if !new.mklabel {
            sort_partitions(&self.partitions, &new.partitions)
        } else {
            (new.partitions.iter().collect(), Vec::new())
        };

        info!("proposed layout:{}", {
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
                        if new.flag_is_enabled(SOURCE) {
                            if source.number != new.number {
                                unreachable!(
                                    "layout validation: wrong number: {} != {}",
                                    new.number, source.number
                                );
                            }

                            if new.flag_is_enabled(REMOVE) {
                                remove_partitions.push(source.start_sector);
                                continue 'outer;
                            }

                            if source.requires_changes(new) {
                                if new.flag_is_enabled(FORMAT) {
                                    remove_partitions.push(source.start_sector);
                                    create_partitions.push(PartitionCreate {
                                        path:         self.device_path.clone(),
                                        start_sector: new.start_sector,
                                        end_sector:   new.end_sector,
                                        format:       true,
                                        file_system:  Some(new.filesystem.expect(
                                            "no file system in partition that requires changes",
                                        )),
                                        kind:         new.part_type,
                                        flags:        new.flags.clone(),
                                        label:        new.name.clone(),
                                    });
                                } else {
                                    change_partitions.push(PartitionChange {
                                        device_path: device_path.clone(),
                                        path:        new.device_path.clone(),
                                        num:         source.number,
                                        kind:        new.part_type,
                                        start:       new.start_sector,
                                        end:         new.end_sector,
                                        filesystem:  source.filesystem,
                                        flags:       flags_diff(
                                            &source.flags,
                                            new.flags.clone().into_iter(),
                                        ),
                                        new_flags:   new.flags.clone(),
                                        label:       new.name.clone(),
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
            if partition.flag_is_enabled(SOURCE) {
                unreachable!("layout validation: extra sources")
            }

            create_partitions.push(PartitionCreate {
                path:         self.device_path.clone(),
                start_sector: partition.start_sector,
                end_sector:   partition.end_sector,
                format:       true,
                file_system:  partition.filesystem,
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
    pub fn commit(&mut self) -> Result<Option<FormatPartitions>, DiskError> {
        info!("committing changes to {}: {:#?}", self.path().display(), self);
        Disk::from_name_with_serial(&self.device_path, &self.serial).and_then(|source| {
            source.diff(self).and_then(|ops| {
                if ops.is_empty() {
                    Ok(None)
                } else {
                    let partitions_to_format = ops
                        .remove()
                        .and_then(|ops| ops.change())
                        .and_then(|ops| ops.create())
                        .map(Some)?;

                    Ok(partitions_to_format)
                }
            })
        })
    }

    /// Reloads the disk information from the disk into our in-memory
    /// representation.
    pub fn reload(&mut self) -> Result<(), DiskError> {
        info!("reloading disk information for {}", self.path().display());

        // Back up any fields that need to be carried over after reloading disk data.
        let collected = self
            .partitions
            .iter_mut()
            .filter_map(|partition| {
                let start = partition.start_sector;
                let name = partition.name.clone();
                let mount = partition.target.as_ref().map(|ref path| path.to_path_buf());
                let vg = partition.lvm_vg.clone();
                let enc = partition.encryption.clone();
                let keyid = partition.key_id.as_ref().cloned();
                let mut subvolumes = std::collections::HashMap::new();

                let format = if partition.flag_is_enabled(FORMAT) && enc.is_some() {
                    Some(FORMAT)
                } else {
                    None
                };

                std::mem::swap(&mut subvolumes, &mut partition.subvolumes);

                if mount.is_some() || name.is_some() || enc.is_some() || vg.is_some() || keyid.is_some() || !subvolumes.is_empty() || format.is_some() {
                    Some((start, name, mount, vg, enc, keyid, subvolumes, format))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // Reload the disk data by re-probing and replacing `self` with the new data.
        *self = Disk::from_name_with_serial(&self.device_path, &self.serial)?;

        // Then re-add the critical information which was lost.
        for (sector, name, mount, vg, enc, keyid, subvolumes, format) in collected {
            info!("checking for mount target at {}", sector);
            let part = self
                .get_partition_at(sector)
                .and_then(|num| self.get_partition_mut(num))
                .expect("partition sectors are off");

            part.name = name;
            part.target = mount;
            part.lvm_vg = vg;
            part.encryption = enc;
            part.key_id = keyid;
            part.subvolumes = subvolumes;

            if let Some(format) = format {
                part.bitflags |= format;
            }
        }

        Ok(())
    }

    pub fn path(&self) -> &Path { &self.device_path }
}
