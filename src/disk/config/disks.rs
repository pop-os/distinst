use super::super::external::{
    cryptsetup_close, cryptsetup_open, lvs, pvs, vgdeactivate, CloseBy
};
use super::super::lvm::{self, generate_unique_id, LvmDevice};
use super::super::mount::{self, swapoff, umount};
use super::super::mounts::{MOUNTS};
use super::super::swaps::{SWAPS};
use super::super::{
    Bootloader, DecryptionError, DiskError, DiskExt, FileSystemType, FileSystemSupport,
    PartitionFlag, PartitionInfo,
};
use super::partitions::{FORMAT, REMOVE, SOURCE};
use super::{detect_fs_on_device, find_partition, find_partition_mut, Disk, LvmEncryption, PartitionTable, PVS};
use libparted::{Device, DeviceType};
use misc::{get_uuid, from_uuid};

use FileSystemType::*;
use itertools::Itertools;
use std::collections::HashSet;
use std::ffi::OsString;
use std::{io, str, thread};
use std::iter::{self, FromIterator};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::borrow::Cow;
use std::ffi::OsStr;

use rayon::prelude::*;
use rayon::iter::IntoParallelRefIterator;

/// A configuration of disks, both physical and logical.
#[derive(Debug, Default, PartialEq)]
pub struct Disks {
    pub(crate) physical: Vec<Disk>,
    pub(crate) logical:  Vec<LvmDevice>,
}

impl Disks {
    /// Adds a disk to the disks configuration.
    pub fn add(&mut self, disk: Disk) {
        self.physical.push(disk);
    }

    pub fn contains_luks(&self) -> bool {
        self.physical
            .iter()
            .flat_map(|d| d.file_system.as_ref().into_iter().chain(d.partitions.iter()))
            .any(|p| p.filesystem == Some(FileSystemType::Luks))
    }

    pub fn get_physical_device<P: AsRef<Path>>(&self, path: P) -> Option<&Disk> {
        self.physical
            .iter()
            .find(|d| d.get_device_path() == path.as_ref())
    }

    pub fn get_physical_device_mut<P: AsRef<Path>>(&mut self, path: P) -> Option<&mut Disk> {
        self.physical
            .iter_mut()
            .find(|d| d.get_device_path() == path.as_ref())
    }

    /// Returns a slice of physical disks stored within the configuration.
    pub fn get_physical_devices(&self) -> &[Disk] { &self.physical }

    /// Returns a mutable slice of physical disks stored within the
    /// configuration.
    pub fn get_physical_devices_mut(&mut self) -> &mut [Disk] { &mut self.physical }

    /// Returns the physical device that contains the partition at path.
    pub fn get_physical_device_with_partition_mut<P: AsRef<Path>>(
        &mut self,
        path: P
    ) -> Option<&mut Disk> {
        let path = path.as_ref();
        for device in self.get_physical_devices_mut() {
            let mut found = false;

            for part in device.get_partitions_mut() {
                if part.get_device_path() == path {
                    found = true;
                    break
                }
            }

            if found {
                return Some(device);
            }
        }

        None
    }

    /// Uses a boxed iterator to get an iterator over all physical partitions.
    pub fn get_physical_partitions<'a>(&'a self) -> Box<Iterator<Item = &'a PartitionInfo> + 'a> {
        let iterator = self.get_physical_devices().iter()
            .flat_map(|disk| {
                let iterator: Box<Iterator<Item = &PartitionInfo>> = if let Some(ref fs) = disk.file_system {
                    Box::new(iter::once(fs).chain(disk.partitions.iter()))
                } else {
                    Box::new(disk.partitions.iter())
                };
                iterator
            });

        Box::new(iterator)
    }

    /// Searches for a LVM device by the LVM volume group name.
    pub fn get_logical_device(&self, group: &str) -> Option<&LvmDevice> {
        self.logical.iter().find(|d| d.volume_group == group)
    }

    /// Searches for a LVM device by the LVM volume group name.
    pub fn get_logical_device_mut(&mut self, group: &str) -> Option<&mut LvmDevice> {
        self.logical.iter_mut().find(|d| d.volume_group == group)
    }

    /// Searches for a LVM device which is inside of the given LUKS physical volume name.
    pub fn get_logical_device_within_pv(&self, pv: &str) -> Option<&LvmDevice> {
        self.logical.iter().find(|d| {
            d.encryption
                .as_ref()
                .map_or(false, |enc| enc.physical_volume == pv)
        })
    }

    /// Searches for a LVM device which is inside of the given LUKS physical volume name.
    pub fn get_logical_device_within_pv_mut(&mut self, pv: &str) -> Option<&mut LvmDevice> {
        self.logical.iter_mut().find(|d| {
            d.encryption
                .as_ref()
                .map_or(false, |enc| enc.physical_volume == pv)
        })
    }

    /// Returns a slice of logical disks stored within the configuration.
    pub fn get_logical_devices(&self) -> &[LvmDevice] { &self.logical }

    /// Returns a mutable slice of logical disks stored within the
    /// configuration.
    pub fn get_logical_devices_mut(&mut self) -> &mut [LvmDevice] { &mut self.logical }

    /// Uses a boxed iterator to get an iterator over all logical partitions.
    pub fn get_logical_partitions<'a>(&'a self) -> Box<Iterator<Item = &'a PartitionInfo> + 'a> {
        let iterator = self.get_logical_devices().iter()
            .flat_map(|disk| {
                let iterator: Box<Iterator<Item = &PartitionInfo>> = if let Some(ref fs) = disk.file_system {
                    Box::new(iter::once(fs).chain(disk.partitions.iter()))
                } else {
                    Box::new(disk.partitions.iter())
                };
                iterator
            });

        Box::new(iterator)
    }

    /// Get all partitions across all physical and logical devices.
    pub fn get_partitions<'a>(&'a self) -> Box<Iterator<Item = &'a PartitionInfo> + 'a> {
        Box::new(self.get_physical_partitions().chain(self.get_logical_partitions()))
    }

    /// Returns a list of device paths which will be modified by this
    /// configuration.
    pub fn get_device_paths_to_modify(&self) -> Vec<PathBuf> {
        let mut output = Vec::new();
        for dev in self.get_physical_devices() {
            if dev.mklabel {
                // Devices with this set no longer hold the original source partitions.
                // TODO: Maybe have a backup field with the old partitions?
                let disk = Disk::from_name_with_serial(&dev.device_path, &dev.serial)
                    .expect("no serial physical device");
                for part in disk.get_partitions()
                    .iter()
                    .map(|part| part.get_device_path())
                {
                    output.push(part.to_path_buf());
                }
            } else {
                for part in dev.get_partitions()
                    .iter()
                    .filter(|part| {
                        part.flag_is_enabled(SOURCE) && part.flag_is_enabled(REMOVE | FORMAT)
                    })
                    .map(|part| part.get_device_path())
                {
                    output.push(part.to_path_buf());
                }
            }
        }

        output
    }

    /// Obtains the partition which contains the given target.
    pub fn get_partition_with_target(&self, target: &Path) -> Option<&PartitionInfo> {
        self.get_partitions()
            .find(|part| part.target.as_ref().map_or(false, |p| p.as_path() == target))
    }

    /// Obtains the partition which contains the given device path
    pub fn get_partition_by_path<P: AsRef<Path>>(&self, target: P) -> Option<&PartitionInfo> {
        self.get_partitions()
            .find(|part| part.get_device_path() == target.as_ref())
    }

    /// Obtains the partition which contains the given device path
    pub fn get_partition_by_path_mut<P: AsRef<Path>>(&mut self, target: P) -> Option<&mut PartitionInfo> {
        self.physical.iter_mut().flat_map(|dev| dev.get_partitions_mut())
            .chain(self.logical.iter_mut().flat_map(|dev| dev.get_partitions_mut()))
            .find(|part| part.get_device_path() == target.as_ref())
    }

    pub fn get_partition_by_uuid<U: AsRef<str>>(&self, target: U) -> Option<&PartitionInfo> {
        from_uuid(target.as_ref()).and_then(|ref target| self.get_partition_by_path(target))
    }

    pub fn get_partition_by_uuid_mut<U: AsRef<str>>(&mut self, target: U) -> Option<&mut PartitionInfo> {
        from_uuid(target.as_ref()).and_then(move |ref target| self.get_partition_by_path_mut(target))
    }

    /// Reports file systems that need to be supported in the install.
    pub fn get_support_flags(&self) -> FileSystemSupport {
        let mut flags = FileSystemSupport::empty();

        for partition in self.get_partitions() {
            match partition.filesystem {
                Some(Btrfs) => flags |= FileSystemSupport::BTRFS,
                Some(Ext2) | Some(Ext3) | Some(Ext4) => flags |= FileSystemSupport::EXT4,
                Some(F2fs) => flags |= FileSystemSupport::F2FS,
                Some(Fat16) | Some(Fat32) => flags |= FileSystemSupport::FAT,
                Some(Ntfs) => flags |= FileSystemSupport::NTFS,
                Some(Xfs) => flags |= FileSystemSupport::XFS,
                Some(Luks) => flags |= FileSystemSupport::LUKS,
                Some(Lvm) => flags |= FileSystemSupport::LVM,
                _ => continue
            };
        }

        flags
    }

    /// Deactivates all device maps associated with the inner disks/partitions
    /// to be modified.
    pub fn deactivate_device_maps(&self) -> Result<(), DiskError> {
        let mounts = MOUNTS.read().expect("failed to get mounts in deactivate_device_maps");
        let swaps = SWAPS.read().expect("failed to get swaps in deactivate_device_maps");
        let umount = move |vg: &str| -> Result<(), DiskError> {
            for lv in lvs(vg).map_err(|why| DiskError::ExternalCommand { why })? {
                if let Some(mount) = mounts.get_mount_point(&lv) {
                    info!(
                        "unmounting logical volume mounted at {}",
                        mount.display()
                    );
                    umount(&mount, false).map_err(|why| DiskError::Unmount {
                        device: lv,
                        why
                    })?;
                } else if let Ok(lv) = lv.canonicalize() {
                    if swaps.get_swapped(&lv) {
                        swapoff(&lv).map_err(|why| DiskError::Unmount {
                            device: lv,
                            why
                        })?;
                    }
                }
            }

            Ok(())
        };

        let devices_to_modify = self.get_device_paths_to_modify();
        info!("devices to modify: {:?}", devices_to_modify);
        let volume_map = pvs().map_err(|why| DiskError::ExternalCommand { why })?;
        info!("volume map: {:?}", volume_map);
        let pvs = lvm::physical_volumes_to_deactivate(&devices_to_modify);
        info!("pvs: {:?}", pvs);

        // Handle LVM on LUKS
        pvs.par_iter().map(|pv| {
            let dev = CloseBy::Path(&pv);
            match volume_map.get(pv) {
                Some(&Some(ref vg)) => umount(vg).and_then(|_| {
                    vgdeactivate(vg)
                        .and_then(|_| cryptsetup_close(dev))
                        .map_err(|why| DiskError::ExternalCommand { why })
                }),
                Some(&None) => {
                    cryptsetup_close(dev).map_err(|why| DiskError::ExternalCommand { why })
                }
                None => Ok(()),
            }
        }).collect::<Result<(), DiskError>>()?;

        // Handle LVM without LUKS
        devices_to_modify
            .iter()
            .filter_map(|dev| volume_map.get(dev))
            .unique()
            .map(|entry| {
                if let Some(ref vg) = *entry {
                    umount(vg)
                        .and_then(|_| vgdeactivate(vg).map_err(|why| DiskError::ExternalCommand { why }))
                } else {
                    Ok(())
                }
            }).collect::<Result<(), DiskError>>()
    }

    /// Attempts to decrypt the specified partition.
    ///
    /// If successful, the new device will be added as a logical disk.
    /// At the moment, only LVM on LUKS configurations are supported here.
    /// LUKS on LUKS, or Something on LUKS, will simply error.
    pub fn decrypt_partition(
        &mut self,
        path: &Path,
        enc: &LvmEncryption,
    ) -> Result<(), DecryptionError> {
        info!("decrypting partition at {:?}", path);
        // An intermediary value that can avoid the borrowck issue.
        let mut new_device = None;

        fn decrypt(
            partition: &mut PartitionInfo,
            path: &Path,
            enc: &LvmEncryption,
        ) -> Result<LvmDevice, DecryptionError> {
            // Attempt to decrypt the device.
            cryptsetup_open(path, &enc).map_err(|why| DecryptionError::Open {
                device: path.to_path_buf(),
                why,
            })?;

            // Determine which VG the newly-decrypted device belongs to.
            let pv = &PathBuf::from(["/dev/mapper/", &enc.physical_volume].concat());
            let mut attempt = 0;
            while !pv.exists() && attempt < 10 {
                info!("waiting 1 second for {:?} to activate", pv);
                attempt += 1;
                thread::sleep(Duration::from_millis(1000));
            }

            match pvs().expect("pvs() failed in decrypt_partition").remove(pv) {
                Some(Some(vg)) => {
                    // Set values in the device's partition.
                    partition.volume_group = Some((vg.clone(), Some(enc.clone())));

                    Ok(LvmDevice::new(vg, Some(enc.clone()), partition.sectors(), 512, true))
                }
                _ => {
                    // Detect a file system on the device
                    if let Some(fs) = detect_fs_on_device(&pv) {
                        let pv = enc.physical_volume.clone();
                        let mut luks = LvmDevice::new(
                            pv,
                            Some(enc.clone()),
                            partition.sectors(),
                            512,
                            true
                        );

                        luks.set_file_system(fs);
                        luks.set_luks_parent(path.to_path_buf());

                        return Ok(luks);
                    }

                    // Attempt to close the device as we've failed to find a VG.
                    let _ = cryptsetup_close(CloseBy::Path(&pv));

                    // NOTE: Should we handle this in some way?
                    Err(DecryptionError::DecryptedLacksVG { device: path.to_path_buf() })
                }
            }
        }

        // Attempt to find the device in the configuration.
        for device in &mut self.physical {
            // TODO: NLL
            if let Some(partition) = device.get_file_system_mut() {
                if partition.device_path == path {
                    decrypt(partition, path, &enc)?;
                }
            }

            for partition in device.file_system.as_mut().into_iter().chain(device.partitions.iter_mut()) {
                if partition.device_path == path {
                    new_device = Some(decrypt(partition, path, &enc)?);
                    break
                }
            }
        }

        match new_device {
            // Add the new LVM device to the disk configuration
            Some(mut device) => {
                if device.file_system.is_none() {
                    device.add_partitions();
                }

                self.logical.push(device);
                Ok(())
            }
            None => Err(DecryptionError::LuksNotFound {
                device: path.to_path_buf(),
            }),
        }
    }

    /// Sometimes, physical devices themselves may be mounted directly.
    pub fn unmount_devices(&self) -> Result<(), DiskError> {
        info!("unmounting devices");
        self.physical.iter().map(|device| {
            if let Some(mount) = device.get_mount_point() {
                if mount != Path::new("/cdrom") {
                    info!(
                        "unmounting device mounted at {}",
                        mount.display()
                    );
                    mount::umount(&mount, false).map_err(|why| DiskError::Unmount {
                        device: device.get_device_path().to_path_buf(),
                        why
                    })?;
                }
            }

            Ok(())
        }).collect::<Result<(), DiskError>>()
    }

    /// Probes for and returns disk information for every disk in the system.
    pub fn probe_devices() -> Result<Disks, DiskError> {
        let mut disks = Disks::default();
        for mut device in Device::devices(true) {
            match device.type_() {
                DeviceType::PED_DEVICE_UNKNOWN
                | DeviceType::PED_DEVICE_LOOP
                | DeviceType::PED_DEVICE_FILE
                | DeviceType::PED_DEVICE_DM => continue,
                _ => disks.add(Disk::new(&mut device, false)?),
            }
        }

        // Collect all of the extended partition information for each contained
        // partition in parallel.
        let mounts = MOUNTS.read().expect("failed to get mounts in Disk::new");
        let swaps = SWAPS.read().expect("failed to get swaps in Disk::new");

        unsafe {
            if PVS.is_none() {
                PVS = Some(pvs().expect("do you have the `lvm2` package installed?"));
            }
        }

        disks.physical.par_iter_mut()
            .flat_map(|device| device.get_partitions_mut())
            .for_each(|part| {
                part.collect_extended_information(&mounts, &swaps);
            });

        Ok(disks)
    }

    /// Returns an immutable reference to the disk specified by its path, if it
    /// exists.
    pub fn find_disk<P: AsRef<Path>>(&self, path: P) -> Option<&Disk> {
        self.physical
            .iter()
            .find(|disk| disk.device_path == path.as_ref())
    }

    /// Returns a mutable reference to the disk specified by its path, if it
    /// exists.
    pub fn find_disk_mut<P: AsRef<Path>>(&mut self, path: P) -> Option<&mut Disk> {
        self.physical
            .iter_mut()
            .find(|disk| disk.device_path == path.as_ref())
    }

    /// Finds the partition block path and associated partition information that is associated with
    /// the given target mount point. Scans both physical and logical partitions.
    pub fn find_partition<'a>(&'a self, target: &Path) -> Option<(&'a Path, &'a PartitionInfo)> {
        find_partition(&self.physical, target).or_else(|| find_partition(&self.logical, target))
    }

    /// Finds the partition block path and associated partition information that is associated with
    /// the given target mount point. Scans both physical and logical partitions. Mutable variant.
    pub fn find_partition_mut<'a>(
        &'a mut self,
        target: &Path,
    ) -> Option<(PathBuf, &'a mut PartitionInfo)> {
        match find_partition_mut(&mut self.physical, target) {
            partition @ Some(_) => partition,
            None => find_partition_mut(&mut self.logical, target)
        }
    }

    /// Returns a list of disk & partition paths that match a volume group.
    pub fn find_volume_paths<'a>(&'a self, volume_group: &str) -> Vec<(&'a Path, &'a Path)> {
        let mut volumes = Vec::new();

        for disk in &self.physical {
            for partition in disk.get_partitions() {
                // The volume group may be stored in either the `original_vg`
                // or `volume_group` fields. This combines the optionals.
                let vg: Option<&String> = partition
                    .volume_group
                    .as_ref()
                    .map(|x| &x.0)
                    .or_else(|| partition.original_vg.as_ref());

                if let Some(ref pvg) = vg {
                    if pvg.as_str() == volume_group {
                        volumes.push((disk.get_device_path(), partition.get_device_path()));
                    }
                }
            }
        }

        volumes
    }

    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub fn get_encrypted_partitions(&self) -> Vec<&PartitionInfo> {
        // Get an iterator on physical partitions
        self.get_physical_devices().iter().flat_map(|d| d.get_partitions().iter())
            // Chain the logical partitions to the iterator
            .chain(self.get_logical_devices().iter().flat_map(|d| d.get_partitions().iter()))
            // Then collect all partitions whose file system is LUKS
            .filter(|p| p.filesystem.map_or(false, |fs| fs == FileSystemType::Luks))
            // Commit
            .collect()
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
                let boot = self.find_partition(Path::new("/boot"));

                let root = self.find_partition(Path::new("/")).expect(
                    "verify_partitions() should have ensured that a root partition was created",
                );

                (root, boot)
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
        info!("verifying if keyfiles have paths");
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
                        if let Some(ref pkey_id) = partition.key_id {
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

    /// Maps key paths to their keyfile IDs TODO
    fn resolve_keyfile_paths(&mut self) -> Result<(), DiskError> {
        let mut temp: Vec<(String, Option<(PathBuf, PathBuf)>)> = Vec::new();

        'outer: for logical_device in &mut self.logical {
            if let Some(ref mut encryption) = logical_device.encryption {
                if let Some((ref key_id, ref mut paths)) = encryption.keydata {
                    let partitions = self.physical.iter()
                        .flat_map(|p| p.file_system.as_ref().into_iter().chain(p.partitions.iter()));
                    for partition in partitions {
                        let dev = partition.get_device_path();
                        if let Some(ref pkey_id) = partition.key_id {
                            match partition.target {
                                Some(ref pkey_mount) => if pkey_id == key_id {
                                    *paths = Some((dev.into(), pkey_mount.into()));
                                    temp.push((pkey_id.clone(), paths.clone()));
                                    continue 'outer;
                                },
                                None => {
                                    return Err(DiskError::KeyFileWithoutPath);
                                }
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
                        if *id == *key {
                            *ppath = paths.clone();
                            continue;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn device_is_logical(&self, device: &Path) -> bool {
        self.get_logical_devices()
            .iter()
            .any(|d| d.get_device_path() == device)
    }

    /// Validates that partitions are configured correctly.
    ///
    /// - EFI installs must contain a `/boot/efi` partition as Fat16 / Fat32
    /// - MBR installs on logical devices must have a `/boot` partition
    /// - Boot partitions must not be on a logical volume
    /// - EFI boot partitions must have the ESP flag set
    pub fn verify_partitions(&self, bootloader: Bootloader) -> io::Result<()> {
        let (root_device, root) = self.find_partition(Path::new("/")).ok_or_else(|| {
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

        let boot_partition = if bootloader == Bootloader::Efi {
            Some(("/boot/efi", "EFI", true))
        } else if self.device_is_logical(root_device) {
            Some(("/boot", "boot", false))
        } else {
            None
        };

        if let Some((partition, kind, is_efi)) = boot_partition {
            let device = {
                let (device, boot) = self.find_partition(Path::new(partition)).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("{} partition was not defined", kind),
                    )
                })?;

                let device = match self.find_disk(device) {
                    Some(device) => device,
                    None => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Unable to find the disk that the boot partition exists on"
                        ))
                    }
                };

                if is_efi {
                    // Check if the EFI partition is on a GPT disk.
                    if device.get_table_type() != Some(PartitionTable::Gpt) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "EFI installs cannot be done on disks without a GPT partition layout."
                        ));
                    }

                    if !boot.flags.contains(&PartitionFlag::PED_PARTITION_ESP) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("{} partition did not have ESP flag set", kind),
                        ));
                    }

                    match boot.filesystem {
                        Some(Fat16) | Some(Fat32) => (),
                        Some(_) => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("{} partition has invalid file system", kind),
                            ));
                        }
                        None => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("{} partition does not have a file system", kind),
                            ));
                        }
                    }

                    // 256 MiB should be the minimal size of the ESP partition.
                    const REQUIRED_SECTORS: u64 = 524_288;

                    if boot.sectors() < REQUIRED_SECTORS {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "the ESP partition must be at least 256 MiB in size"
                        ));
                    }
                }

                device
            };

            if device.is_logical() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{} partition cannot be on logical device", kind),
                ));
            }
        }

        Ok(())
    }

    /// Generates the crypttab and fstab files in memory.
    pub(crate) fn generate_fstabs(&self) -> (OsString, OsString) {
        info!("generating /etc/crypttab & /etc/fstab in memory");
        let mut crypttab = OsString::with_capacity(1024);
        let mut fstab = OsString::with_capacity(1024);

        let partitions = self.physical
            .iter()
            .flat_map(|x| {
                x.file_system.as_ref().into_iter()
                    .chain(x.partitions.iter())
                    .map(|p| (true, &None, p))
            })
            .chain(self.logical.iter().flat_map(|x| {
                let luks_parent = &x.luks_parent;
                let is_unencrypted: bool = x.encryption.is_none();
                x.file_system.as_ref().into_iter()
                    .chain(x.partitions.iter())
                    .map(move |p| (is_unencrypted, luks_parent, p))
            }));

        fn write_fstab(fstab: &mut OsString, partition: &PartitionInfo) {
            if let Some(entry) = partition.get_block_info() {
                entry.write_fstab(fstab);
            }
        }

        for (is_unencrypted, luks_parent, partition) in partitions {
            if let Some(&(_, Some(ref enc))) = partition.volume_group.as_ref() {
                let password: Cow<'static, OsStr> =
                    match (enc.password.is_some(), enc.keydata.as_ref()) {
                        (true, None) => Cow::Borrowed(OsStr::new("none")),
                        (false, None) => Cow::Borrowed(OsStr::new("/dev/urandom")),
                        (true, Some(_key)) => unimplemented!(),
                        (false, Some(&(_, ref key))) => {
                            let path = key.clone()
                                .expect("should have been populated")
                                .1
                                .join(&enc.physical_volume);
                            Cow::Owned(path.into_os_string())
                        }
                    };

                let path = luks_parent.as_ref().map_or(&partition.device_path, |x| &x);

                match get_uuid(path) {
                    Some(uuid) => {
                        crypttab.push(&enc.physical_volume);
                        crypttab.push(" UUID=");
                        crypttab.push(&uuid);
                        crypttab.push(" ");
                        crypttab.push(&password);
                        crypttab.push(" luks\n");
                        write_fstab(&mut fstab, &partition);
                    }
                    None => error!(
                        "unable to find UUID for {} -- skipping",
                        partition.device_path.display()
                    ),
                }
            } else if partition.filesystem == Some(FileSystemType::Swap) {
                if is_unencrypted {
                    match get_uuid(&partition.device_path) {
                        Some(uuid) => {
                            let unique_id =
                                generate_unique_id("cryptswap").unwrap_or_else(|_| "cryptswap".into());
                            crypttab.push(&unique_id);
                            crypttab.push(" UUID=");
                            crypttab.push(&uuid);
                            crypttab.push(
                                " /dev/urandom swap,offset=1024,cipher=aes-xts-plain64,size=512\n",
                            );

                            fstab.push(&[
                                "/dev/mapper/",
                                &unique_id,
                                "  none  swap  defaults  0  0\n",
                            ].concat());
                        }
                        None => error!(
                            "unable to find UUID for {} -- skipping",
                            partition.device_path.display()
                        ),
                    }
                } else {
                    fstab.push(partition.get_device_path());
                    fstab.push("  none  swap  defaults  0  0\n");
                }
            } else {
                write_fstab(&mut fstab, &partition);
            }
        }

        info!(
            "generated the following crypttab data:\n{}",
            crypttab.to_string_lossy(),
        );

        info!(
            "generated the following fstab data:\n{}",
            fstab.to_string_lossy()
        );

        crypttab.shrink_to_fit();
        fstab.shrink_to_fit();
        (crypttab, fstab)
    }

    /// Loads existing logical volume data into memory, excluding encrypted volumes.
    pub fn initialize_volume_groups(&mut self) -> Result<(), DiskError> {
        let mut existing_devices: Vec<LvmDevice> = Vec::new();

        for disk in &self.physical {
            let sector_size = disk.get_sector_size();
            for partition in disk.get_partitions().iter() {
                if let Some(ref lvm) = partition.volume_group {
                    // TODO: NLL
                    let push = match existing_devices
                        .iter_mut()
                        .find(|d| d.volume_group == lvm.0)
                    {
                        Some(device) => {
                            device.add_sectors(partition.sectors());
                            false
                        }
                        None => true,
                    };

                    if push {
                        existing_devices.push(LvmDevice::new(
                            lvm.0.clone(),
                            lvm.1.clone(),
                            partition.sectors(),
                            sector_size,
                            false,
                        ));
                    }
                } else if let Some(ref vg) = partition.original_vg {
                    eprintln!(
                        "found existing LVM device on {:?}",
                        partition.get_device_path()
                    );
                    // TODO: NLL
                    let mut found = false;

                    if let Some(ref mut device) = existing_devices
                        .iter_mut()
                        .find(|d| d.volume_group.as_str() == vg.as_str())
                    {
                        device.add_sectors(partition.sectors());
                        found = true;
                    }

                    if !found {
                        existing_devices.push(LvmDevice::new(
                            vg.clone(),
                            None,
                            partition.sectors(),
                            sector_size,
                            true,
                        ));
                    }
                }
            }
        }

        for device in &mut existing_devices {
            if !device.is_source {
                continue;
            }

            device.add_partitions();
        }

        self.logical = existing_devices;

        Ok(())
    }

    pub fn remove_logical_device(&mut self, volume: &str) {
        let mut remove_id = None;
        for (id, device) in self.logical.iter_mut().enumerate() {
            if device.volume_group == volume {
                if device.is_source {
                    device.remove = true;
                } else {
                    remove_id = Some(id);
                }
                break;
            }
        }

        if let Some(id) = remove_id {
            let _ = self.logical.remove(id);
        }
    }

    /// Applies all logical device operations, which are to be performed after all physical disk
    /// operations have completed.
    ///
    /// TODO: We need to generate a diff of logical volume operations.
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
            for partition in device.file_system.as_mut().into_iter().chain(device.partitions.iter_mut()) {
                // ... unless it is populated, due to existing beforehand.
                if partition.flag_is_enabled(SOURCE) {
                    continue;
                }
                let label = partition.name.as_ref().expect("logical partition should have name");
                partition.device_path =
                    PathBuf::from(format!("/dev/mapper/{}-{}", device.volume_group, label));
            }
        }

        // Ensure that the keyfile paths are mapped to their mount targets.
        self.resolve_keyfile_paths()?;

        // Now we will apply the logical layout.
        for device in &self.logical {
            // Only create the device if it does not exist.
            if !device.is_source {
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
            }

            device.modify_partitions()?;
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
