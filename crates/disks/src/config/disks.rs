use super::{
    super::{
        Bootloader, DecryptionError, DiskError, DiskExt, FileSystem, LogicalDevice, PartitionFlag,
        PartitionInfo,
    },
    detect_fs_on_device, find_partition, find_partition_mut,
    partitions::{FORMAT, REMOVE, SOURCE},
    Disk, LuksEncryption, PartitionTable, PVS,
};
use disk_types::{BlockDeviceExt, PartitionExt, PartitionTableExt, SectorExt};
use crate::external::{
    cryptsetup_close, cryptsetup_open, lvs, physical_volumes_to_deactivate, pvs, vgdeactivate,
    CloseBy,
};
use itertools::Itertools;
use libparted::{Device, DeviceType};
use partition_identity::PartitionID;
use proc_mounts::{MountIter, MOUNTS, SWAPS};
use rayon::{iter::IntoParallelRefIterator, prelude::*};
use std::{
    collections::{BTreeMap, HashSet},
    ffi::OsString,
    fs, io,
    iter::{self, FromIterator},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    str, thread,
    time::Duration,
};
use sys_mount::{swapoff, unmount, Mount, MountFlags, Mounts, Unmount, UnmountFlags};

/// A configuration of disks, both physical and logical.
#[derive(Debug, Default, PartialEq)]
pub struct Disks {
    pub physical: Vec<Disk>,
    pub logical:  Vec<LogicalDevice>,
}

impl Disks {
    /// Adds a disk to the disks configuration.
    pub fn add(&mut self, disk: Disk) { self.physical.push(disk); }

    /// Remove disks that aren't relevant to the install.
    pub fn remove_untouched_disks(&mut self) {
        let mut remove = Vec::with_capacity(self.physical.len() - 1);

        for (id, disk) in self.physical.iter().enumerate() {
            if !disk.is_being_modified() {
                debug!(
                    "removing {:?} from consideration: no action to apply",
                    disk.get_device_path()
                );
                remove.push(id);
            }
        }

        remove.into_iter().rev().for_each(|id| {
            self.physical.remove(id);
        });
    }

    pub fn contains_luks(&self) -> bool {
        self.physical
            .iter()
            .flat_map(|d| d.file_system.as_ref().into_iter().chain(d.partitions.iter()))
            .any(|p| p.filesystem == Some(FileSystem::Luks))
    }

    pub fn get_physical_device<P: AsRef<Path>>(&self, path: P) -> Option<&Disk> {
        self.physical.iter().find(|d| d.get_device_path() == path.as_ref())
    }

    pub fn get_physical_device_mut<P: AsRef<Path>>(&mut self, path: P) -> Option<&mut Disk> {
        self.physical.iter_mut().find(|d| d.get_device_path() == path.as_ref())
    }

    /// Returns a slice of physical disks stored within the configuration.
    pub fn get_physical_devices(&self) -> &[Disk] { &self.physical }

    /// Returns a mutable slice of physical disks stored within the
    /// configuration.
    pub fn get_physical_devices_mut(&mut self) -> &mut [Disk] { &mut self.physical }

    /// Returns the physical device that contains the partition at path.
    pub fn get_physical_device_with_partition<P: AsRef<Path>>(&self, path: P) -> Option<&Disk> {
        let path = path.as_ref();
        self.get_physical_devices().iter().find(|device| {
            device.get_device_path() == path
                || device.get_partitions().iter().any(|p| p.get_device_path() == path)
        })
    }

    /// Returns the physical device, mutably, that contains the partition at path.
    pub fn get_physical_device_with_partition_mut<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Option<&mut Disk> {
        let path = path.as_ref();
        self.get_physical_devices_mut().iter_mut().find(|device| {
            device.get_device_path() == path
                || device.get_partitions().iter().any(|p| p.get_device_path() == path)
        })
    }

    /// Uses a boxed iterator to get an iterator over all physical partitions.
    pub fn get_physical_partitions<'a>(&'a self) -> Box<dyn Iterator<Item = &'a PartitionInfo> + 'a> {
        let iterator = self.get_physical_devices().iter().flat_map(|disk| {
            let iterator: Box<dyn Iterator<Item = &PartitionInfo>> =
                if let Some(ref fs) = disk.file_system {
                    Box::new(iter::once(fs).chain(disk.partitions.iter()))
                } else {
                    Box::new(disk.partitions.iter())
                };
            iterator
        });

        Box::new(iterator)
    }

    /// Searches for a LVM device by the LVM volume group name.
    pub fn get_logical_device(&self, group: &str) -> Option<&LogicalDevice> {
        self.logical.iter().find(|d| d.volume_group == group)
    }

    /// Searches for a LVM device by the LVM volume group name.
    pub fn get_logical_device_mut(&mut self, group: &str) -> Option<&mut LogicalDevice> {
        self.logical.iter_mut().find(|d| d.volume_group == group)
    }

    /// Searches for a LVM device which is inside of the given LUKS physical volume name.
    pub fn get_logical_device_within_pv(&self, pv: &str) -> Option<&LogicalDevice> {
        self.logical
            .iter()
            .find(|d| d.encryption.as_ref().map_or(false, |enc| enc.physical_volume == pv))
    }

    /// Searches for a LVM device which is inside of the given LUKS physical volume name.
    pub fn get_logical_device_within_pv_mut(&mut self, pv: &str) -> Option<&mut LogicalDevice> {

        self.logical
            .iter_mut()
            .find(|d| d.encryption.as_ref().map_or(false, |enc| enc.physical_volume == pv))
    }

    /// Returns a slice of logical disks stored within the configuration.
    pub fn get_logical_devices(&self) -> &[LogicalDevice] { &self.logical }

    /// Returns a mutable slice of logical disks stored within the
    /// configuration.
    pub fn get_logical_devices_mut(&mut self) -> &mut [LogicalDevice] { &mut self.logical }

    /// Uses a boxed iterator to get an iterator over all logical partitions.
    pub fn get_logical_partitions<'a>(&'a self) -> Box<dyn Iterator<Item = &'a PartitionInfo> + 'a> {
        let iterator = self.get_logical_devices().iter().flat_map(|disk| {
            let iterator: Box<dyn Iterator<Item = &PartitionInfo>> =
                if let Some(ref fs) = disk.file_system {
                    Box::new(iter::once(fs).chain(disk.partitions.iter()))
                } else {
                    Box::new(disk.partitions.iter())
                };
            iterator
        });

        Box::new(iterator)
    }

    /// Mounts all targets in this disks object.
    pub fn mount_all_targets<P: AsRef<Path>>(&self, base_dir: P) -> io::Result<Mounts> {
        let base_dir = base_dir.as_ref();
        let targets =
            self.get_partitions().filter(|part| {
                (part.target.is_some() && part.filesystem.is_some())
                    || !part.subvolumes.is_empty()
            });

        #[derive(Debug)]
        enum MountKind {
            Direct { data: String, device: PathBuf, fs: &'static str },
            Bind { source: PathBuf },
        }

        // The mount path will actually consist of the target concatenated with the
        // root. NOTE: It is assumed that the target is an absolute path.
        let paths: BTreeMap<PathBuf, MountKind> = targets
            .flat_map(|partition| {
                let generate_target = |part: &PartitionInfo, data: String, target: &Path| {
                    // Path mangling commences here, since we need to concatenate an absolute
                    // path onto another absolute path, and the standard library opts for
                    // overwriting the original path when doing that.
                    let target_mount: PathBuf = {
                        // Ensure that the base_dir path has the ending '/'.
                        let base_dir = base_dir.as_os_str().as_bytes();
                        let mut target_mount: Vec<u8> = if base_dir[base_dir.len() - 1] == b'/' {
                            base_dir.to_owned()
                        } else {
                            let mut temp = base_dir.to_owned();
                            temp.push(b'/');
                            temp
                        };

                        // Cut the starting '/' from the target path if it exists.
                        let target_path = target.as_os_str().as_bytes();
                        let target_path = if !target_path.is_empty() && target_path[0] == b'/' {
                            if target_path.len() > 1 {
                                &target_path[1..]
                            } else {
                                b""
                            }
                        } else {
                            target_path
                        };

                        // Append the target path to the base_dir, and return it as a path type.
                        target_mount.extend_from_slice(target_path);
                        PathBuf::from(OsString::from_vec(target_mount))
                    };

                    // If a partition is already mounted, we should perform a bind mount.
                    // If it is not mounted, we can mount it directly.
                    let kind = if let Some(source) = part.mount_point.get(0).cloned() {
                        MountKind::Bind { source }
                    } else {
                        let device;
                        let fs;

                        match part.filesystem.unwrap() {
                            FileSystem::Fat16 | FileSystem::Fat32 => {
                                fs = "vfat";
                                device = part.device_path.clone();
                            }
                            FileSystem::Luks => {
                                let encryption = part.encryption.as_ref().unwrap();
                                fs = encryption.filesystem.into();
                                device = PathBuf::from(["/dev/mapper/", &*encryption.physical_volume].concat());
                            }
                            other => {
                                fs = other.into();
                                device = part.device_path.clone();
                            },
                        };

                        MountKind::Direct { data, device, fs }
                    };

                    (target_mount, kind)
                };

                if !partition.subvolumes.is_empty() {
                    partition.subvolumes.iter()
                        .map(|(target, subvol)| {
                            generate_target(partition, format!("subvol={}", subvol), target)
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![generate_target(partition, String::new(), partition.target.as_deref().unwrap())]
                }
            })
            .collect();

        // Each mount directory will be created and then mounted before progressing to
        // the next mount in the map. The BTreeMap that the mount targets were
        // collected into will ensure that mounts are created and mounted in
        // the correct order.
        let mut mounts = Vec::new();

        for (target_mount, kind) in paths {
            if let Err(why) = fs::create_dir_all(&target_mount) {
                error!("unable to create '{}': {}", why, target_mount.display());
            }

            let mount = match kind {
                MountKind::Direct { data, device, fs } => {
                    info!("mounting {:?} ({}) to {:?} with {}", device, fs, target_mount, data);

                    let mut result = Mount::builder()
                        .fstype(fs)
                        .data(&data)
                        .mount(&device, &target_mount);

                    // Create missing subvolumes with zstd compression.
                    if let Err(io::ErrorKind::NotFound) = result.as_ref().map_err(io::Error::kind) {
                        if let Some(subvol) = data.strip_prefix("subvol=") {
                            const BTRFS_MOUNT: &str = "/tmp/distinst.btrfs/";

                            let _ = fs::create_dir_all(BTRFS_MOUNT);

                            let subvol = &*[BTRFS_MOUNT, subvol].concat();

                            info!("mounting {:?} to {}", device, BTRFS_MOUNT);
                            let mount = Mount::builder()
                                .fstype(fs)
                                .mount(&device, BTRFS_MOUNT)?;

                            info!("creating subvolume at {}", subvol);
                            std::process::Command::new("btrfs")
                                .args(&["subvolume", "create", subvol])
                                .status()?;

                            info!("setting zstd compression on {}", subvol);
                            std::process::Command::new("btrfs")
                                .args(&["property", "set", subvol, "compression", "zstd"])
                                .status()?;

                            let _ = mount.unmount(UnmountFlags::DETACH);

                            result = Mount::builder()
                                .fstype(fs)
                                .data(&data)
                                .mount(&device, &target_mount)
                        }
                    }

                    result?
                }
                MountKind::Bind { source } => {
                    info!("bind mounting {:?} to {:?}", source, target_mount);
                    Mount::new(source, &target_mount, "", MountFlags::BIND, None)?
                }
            };

            mounts.push(mount.into_unmount_drop(UnmountFlags::DETACH));
        }

        Ok(Mounts(mounts))
    }

    /// Get all partitions across all physical and logical devices.
    pub fn get_partitions<'a>(&'a self) -> Box<dyn Iterator<Item = &'a PartitionInfo> + 'a> {
        Box::new(self.get_physical_partitions().chain(self.get_logical_partitions()))
    }

    pub fn get_partitions_mut<'a>(
        &'a mut self,
    ) -> Box<dyn Iterator<Item = &'a mut PartitionInfo> + 'a> {
        Box::new(
            self.physical
                .iter_mut()
                .flat_map(|dev| dev.get_partitions_mut())
                .chain(self.logical.iter_mut().flat_map(|dev| dev.get_partitions_mut())),
        )
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
                for part in disk.get_partitions().iter().map(|part| part.get_device_path()) {
                    output.push(part.to_path_buf());
                }
            } else {
                for part in dev
                    .get_partitions()
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
            .find(|part| {
                part.target.as_ref().map_or(false, |p| p.as_path() == target)
                    || part.subvolumes.get(target).is_some()
            })
    }

    /// Obtains the partition which contains the given device path
    pub fn get_partition_by_path<P: AsRef<Path>>(&self, target: P) -> Option<&PartitionInfo> {
        self.get_partitions()
            .find(|part| misc::canonicalize(part.get_device_path()) == target.as_ref())
    }

    /// Obtains the partition which contains the given device path
    pub fn get_partition_by_path_mut<P: AsRef<Path>>(
        &mut self,
        target: P,
    ) -> Option<&mut PartitionInfo> {
        self.get_partitions_mut()
            .find(|part| misc::canonicalize(part.get_device_path()) == target.as_ref())
    }

    /// Obtains the partition which contains the given identity
    pub fn get_partition_by_id(&self, id: &PartitionID) -> Option<&PartitionInfo> {
        self.get_partitions().find(|part| part.identifiers.matches(id))
    }

    /// Obtains the partition which contains the given identity
    pub fn get_partition_by_id_mut(&mut self, id: &PartitionID) -> Option<&mut PartitionInfo> {
        self.get_partitions_mut().find(|part| {
            part.identifiers.matches(id) || {
                if let Some((path, _)) = part.encrypted_info() {
                    if let Some(encrypted_id) = PartitionID::get_uuid(&path) {
                        if &encrypted_id == id {
                            return true
                        }
                    }
                }

                false
            }
        })
    }

    #[deprecated(note = "use the 'get_partition_by_id()' method instead")]
    pub fn get_partition_by_uuid(&self, target: String) -> Option<&PartitionInfo> {
        PartitionID::new_uuid(target)
            .get_device_path()
            .and_then(|ref target| self.get_partition_by_path(target))
    }

    #[deprecated(note = "use the 'get_partition_by_id_mut()' method instead")]
    pub fn get_partition_by_uuid_mut(&mut self, target: String) -> Option<&mut PartitionInfo> {
        PartitionID::new_uuid(target)
            .get_device_path()
            .and_then(move |ref target| self.get_partition_by_path_mut(target))
    }

    /// Find the disk which contains the given mount.
    pub fn get_disk_with_mount<P: AsRef<Path>>(&self, target: P) -> Option<&Disk> {
        let device_path = find_device_path_of_mount(target).ok()?;
        self.get_physical_device_with_partition(&device_path)
    }

    /// Find the disk, mutably, which contains the given mount.
    pub fn get_disk_with_mount_mut<P: AsRef<Path>>(&mut self, target: P) -> Option<&mut Disk> {
        let device_path = find_device_path_of_mount(target).ok()?;
        self.get_physical_device_with_partition_mut(&device_path)
    }

    /// Find the disk which contains the partition with the given Partition ID.
    pub fn get_disk_with_partition(&self, target: &PartitionID) -> Option<&Disk> {
        self.get_physical_devices()
            .iter()
            .find(|disk| disk.partitions.iter().any(|p| p.identifiers.matches(target)))
    }

    /// Find the disk, mutably, which contains the partition with the given Partition ID.
    pub fn get_disk_with_partition_mut(&mut self, target: &PartitionID) -> Option<&mut Disk> {
        self.get_physical_devices_mut()
            .iter_mut()
            .find(|disk| disk.partitions.iter().any(|p| p.identifiers.matches(target)))
    }

    /// Deactivates all device maps associated with the inner disks/partitions
    /// to be modified.
    pub fn deactivate_device_maps(&self) -> Result<(), DiskError> {
        let mounts = MOUNTS.read().expect("failed to get mounts in deactivate_device_maps");
        let swaps = SWAPS.read().expect("failed to get swaps in deactivate_device_maps");
        let umount = move |vg: &str| -> Result<(), DiskError> {
            for lv in lvs(vg).map_err(|why| DiskError::ExternalCommand { why })? {
                if let Some(mount) = mounts.get_mount_by_source(&lv) {
                    info!("unmounting logical volume mounted at {}", mount.dest.display());
                    unmount(&mount.dest, UnmountFlags::empty())
                        .map_err(|why| DiskError::Unmount { device: lv, why })?;
                } else if let Ok(lv) = lv.canonicalize() {
                    if swaps.get_swapped(&lv) {
                        swapoff(&lv).map_err(|why| DiskError::Unmount { device: lv, why })?;
                    }
                }
            }

            Ok(())
        };

        let devices_to_modify = self.get_device_paths_to_modify();
        info!("devices to modify: {:?}", devices_to_modify);
        let volume_map = pvs().map_err(|why| DiskError::ExternalCommand { why })?;
        info!("volume map: {:?}", volume_map);
        let pvs = physical_volumes_to_deactivate(&devices_to_modify);
        info!("pvs: {:?}", pvs);

        // Handle LVM on LUKS
        pvs.par_iter()
            .map(|pv| {
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
            })
            .collect::<Result<(), DiskError>>()?;

        // Handle LVM without LUKS
        devices_to_modify
            .iter()
            .filter_map(|dev| volume_map.get(dev))
            .unique()
            .try_for_each(|entry| {
                if let Some(ref vg) = *entry {
                    umount(vg).and_then(|_| {
                        vgdeactivate(vg).map_err(|why| DiskError::ExternalCommand { why })
                    })
                } else {
                    Ok(())
                }
            })
    }

    /// Attempts to decrypt the specified partition.
    ///
    /// If successful, the new device will be added as a logical disk.
    /// At the moment, only LVM on LUKS configurations are supported here.
    /// LUKS on LUKS, or Something on LUKS, will simply error.
    pub fn decrypt_partition(
        &mut self,
        path: &Path,
        enc: &mut LuksEncryption,
    ) -> Result<(), DecryptionError> {
        info!("decrypting partition at {:?}", path);
        // An intermediary value that can avoid the borrowck issue.
        let mut new_device = None;

        fn decrypt(
            partition: &mut PartitionInfo,
            path: &Path,
            enc: &mut LuksEncryption,
        ) -> Result<LogicalDevice, DecryptionError> {
            // Attempt to decrypt the device.
            cryptsetup_open(path, enc)
                .map_err(|why| DecryptionError::Open { device: path.to_path_buf(), why })?;

            // Determine which PV the newly-decrypted device belongs to.
            let pv = &PathBuf::from(["/dev/mapper/", &enc.physical_volume].concat());
            info!("which belongs to PV {:?}", pv);
            let mut attempt = 0;
            while !pv.exists() && attempt < 10 {
                info!("waiting 1 second for {:?} to activate", pv);
                attempt += 1;
                thread::sleep(Duration::from_millis(1000));
            }

            match pvs().expect("pvs() failed in decrypt_partition").remove(pv) {
                Some(Some(vg)) => {
                    // Set values in the device's partition.
                    partition.lvm_vg = Some(vg.clone());
                    partition.encryption = Some(enc.clone());

                    let mut luks = LogicalDevice::new(
                        vg,
                        Some(enc.clone()),
                        partition.get_sectors(),
                        512,
                        true,
                    );
                    info!("settings luks_parent to {:?}", path);
                    luks.set_luks_parent(path.to_path_buf());

                    Ok(luks)
                }
                _ => {
                    // Detect a file system on the device
                    if let Some(fs) = detect_fs_on_device(&pv) {
                        let pv = enc.physical_volume.clone();
                        let mut luks = LogicalDevice::new(
                            pv,
                            Some(enc.clone()),
                            partition.get_sectors(),
                            512,
                            true,
                        );

                        if let Some(fs) = fs.filesystem {
                            enc.filesystem = fs;
                        }

                        partition.encryption = Some(enc.clone());

                        luks.set_file_system(fs);
                        info!("settings luks_parent to {:?}", path);
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
                if partition.get_device_path() == path {
                    decrypt(partition, path, enc)?;
                }
            }

            for partition in
                device.file_system.as_mut().into_iter().chain(device.partitions.iter_mut())
            {
                if partition.get_device_path() == path {
                    new_device = Some(decrypt(partition, path, enc)?);
                    break;
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
            None => Err(DecryptionError::LuksNotFound { device: path.to_path_buf() }),
        }
    }

    /// Sometimes, physical devices themselves may be mounted directly.
    pub fn unmount_devices(&self) -> Result<(), DiskError> {
        info!("unmounting devices");
        self.physical
            .iter()
            .try_for_each(|device| {
                if let Some(mount) = device.get_mount_point().get(0) {
                    if mount != Path::new("/cdrom") {
                        info!("unmounting device mounted at {}", mount.display());
                        unmount(&mount, UnmountFlags::empty()).map_err(|why| {
                            DiskError::Unmount {
                                device: device.get_device_path().to_path_buf(),
                                why,
                            }
                        })?;
                    }
                }

                Ok(())
            })
    }

    /// Probes for and returns disk information for every disk in the system.
    pub fn probe_devices() -> Result<Disks, DiskError> {
        info!("probing devices");
        let mut disks = Disks::default();
        for mut device in Device::devices(true) {
            if let Some(name) = device.path().file_name().and_then(|x| x.to_str()) {
                // Ignore CDROM devices
                if name.starts_with("sr") || name.starts_with("scd") { continue }

                info!("probed {:?}", device.path());

                match device.type_() {
                    DeviceType::PED_DEVICE_UNKNOWN
                    | DeviceType::PED_DEVICE_LOOP
                    | DeviceType::PED_DEVICE_FILE
                    | DeviceType::PED_DEVICE_DM => continue,
                    _ => disks.add(Disk::new(&mut device, false)?),
                }
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

        disks.physical.par_iter_mut().flat_map(|device| device.get_partitions_mut()).for_each(
            |part| {
                part.collect_extended_information(&mounts, &swaps);
            },
        );

        Ok(disks)
    }

    /// Locate a partition which contains the given file.
    ///
    /// ```rust
    /// extern crate disk_types;
    /// use disk_types::FileSystem;
    ///
    /// Disks::probe_for(
    ///     "recovery.conf",
    ///     "/recovery",
    ///     |fs| fs == Fat16 || fs == Fat32,
    ///     |recovery_path| {
    ///         let mut file = File::open(recovery_path)?;
    ///         let mut output = String::new();
    ///         file.read_to_end(&mut output)?;
    ///
    ///         println!("Found recovery.conf at {:?}:\n\n{}", recovery_path, output);
    ///         Ok(())
    ///     }
    /// )
    /// ```
    pub fn probe_for<T, E, F, C, S>(
        expected_at: E,
        mut supported: S,
        mut condition: C,
        mut func: F,
    ) -> io::Result<T>
    where
        E: AsRef<Path>,
        S: FnMut(FileSystem) -> bool,
        C: FnMut(&PartitionInfo, &Path) -> bool,
        F: FnMut(&PartitionInfo, &Path) -> T,
    {
        let disks = Disks::probe_devices()?;
        let expected_at = expected_at.as_ref();

        // Check partitions which have already been mounted first.
        for partition in disks.get_physical_partitions() {
            if ! partition.mount_point.is_empty() {
                for path in &partition.mount_point {
                    if path == expected_at {
                        return Ok(func(partition, path))
                    }
                }

                continue
            }

            // match partition.mount_point {
            //     Some(ref path) if path == expected_at => return Ok(func(partition, path)),
            //     Some(_) => continue,
            //     None => (),
            // }
        }

        // Then check partitions which have not been mounted yet.
        for partition in disks.get_physical_partitions() {
            if !partition.mount_point.is_empty() {
                continue;
            }

            let has_filesystem = match partition.filesystem {
                Some(fs) => supported(fs),
                None => false,
            };

            if has_filesystem {
                let result = partition.probe(|mount| {
                    mount.and_then(|(path, _mount)| {
                        if condition(partition, path) {
                            Some(func(partition, path))
                        } else {
                            None
                        }
                    })
                });

                if let Some(result) = result {
                    return Ok(result);
                }
            }
        }

        Err(io::Error::new(io::ErrorKind::NotFound, "partition was not found"))
    }

    /// Returns an immutable reference to the disk specified by its path, if it
    /// exists.
    pub fn find_disk<P: AsRef<Path>>(&self, path: P) -> Option<&Disk> {
        self.physical.iter().find(|disk| disk.device_path == path.as_ref())
    }

    /// Returns a mutable reference to the disk specified by its path, if it
    /// exists.
    pub fn find_disk_mut<P: AsRef<Path>>(&mut self, path: P) -> Option<&mut Disk> {
        self.physical.iter_mut().find(|disk| disk.device_path == path.as_ref())
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
            None => find_partition_mut(&mut self.logical, target),
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
                    .lvm_vg
                    .as_ref()
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

    #[rustfmt::skip]
    pub fn get_encrypted_partitions(&self) -> Vec<&PartitionInfo> {
        // Get an iterator on physical partitions
        self.get_physical_devices().iter().flat_map(|d| d.get_partitions().iter())
            // Chain the logical partitions to the iterator
            .chain(self.get_logical_devices().iter().flat_map(|d| d.get_partitions().iter()))
            // Then collect all partitions whose file system is LUKS
            .filter(|p| p.filesystem.map_or(false, |fs| fs == FileSystem::Luks))
            // Commit
            .collect()
    }

    #[rustfmt::skip]
    pub fn get_encrypted_partitions_mut(&mut self) -> Vec<&mut PartitionInfo> {
        let mut partitions = Vec::new();

        let physical = &mut self.physical;
        let logical = &mut self.logical;

        for partition in physical.iter_mut().flat_map(|d| d.get_partitions_mut().iter_mut()) {
            if partition.filesystem.map_or(false, |fs| fs == FileSystem::Luks) {
                partitions.push(partition);
            }
        }

        for partition in logical.iter_mut().flat_map(|d| d.get_partitions_mut().iter_mut()) {
            if partition.filesystem.map_or(false, |fs| fs == FileSystem::Luks) {
                partitions.push(partition);
            }
        }

        partitions
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

    pub fn set_subvolume(&mut self, volume: &PartitionID, name: &str, target: impl AsRef<Path>) {
        if let Some(partition) = self.get_partition_by_id_mut(volume) {
            partition.subvolumes.insert(PathBuf::from(target.as_ref()), name.into());
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
                                    return Err(DiskError::KeyPathAlreadySet {
                                        id: key_id.clone(),
                                    });
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
                    let partitions = self.physical.iter().flat_map(|p| {
                        p.file_system.as_ref().into_iter().chain(p.partitions.iter())
                    });
                    for partition in partitions {
                        let dev = partition.get_device_path();
                        if let Some(ref pkey_id) = partition.key_id {
                            match partition.target {
                                Some(ref pkey_mount) => {
                                    if pkey_id == key_id {
                                        *paths = Some((dev.into(), pkey_mount.into()));
                                        temp.push((pkey_id.clone(), paths.clone()));
                                        continue 'outer;
                                    }
                                }
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
            let partitions = self
                .physical
                .iter_mut()
                .flat_map(|x| x.get_partitions_mut().iter_mut())
                .chain(self.logical.iter_mut().flat_map(|x| x.get_partitions_mut().iter_mut()));

            for partition in partitions {
                if let Some(ref mut enc) = partition.encryption.as_mut() {
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
        self.get_logical_devices().iter().any(|d| d.get_device_path() == device)
    }

    /// Validates that partitions are configured correctly.
    ///
    /// - EFI installs must contain a `/boot/efi` partition as Fat16 / Fat32
    /// - MBR installs on logical devices must have a `/boot` partition
    /// - Boot partitions must not be on a logical volume
    /// - EFI boot partitions must have the ESP flag set
    pub fn verify_partitions(&self, bootloader: Bootloader) -> io::Result<()> {
        let (root_device, root) = self.find_partition(Path::new("/")).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "root partition was not defined")
        })?;

        use FileSystem::*;
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
                let (device, boot) =
                    self.find_partition(Path::new(partition)).ok_or_else(|| {
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
                            "Unable to find the disk that the boot partition exists on",
                        ))
                    }
                };

                if is_efi {
                    // Check if the EFI partition is on a GPT disk.
                    if device.get_partition_table() != Some(PartitionTable::Gpt) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "EFI installs cannot be done on disks without a GPT partition layout.",
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
                    const REQUIRED_ESP_SIZE: u64 = 256 * 1024 * 1024;
                    const REQUIRED_SECTORS: u64 = 524_288;

                    if (boot.get_device_path().read_link().is_err()
                        && boot.get_sectors() < REQUIRED_SECTORS)
                        || (boot.get_device_path().read_link().is_ok()
                            && (boot.get_sectors() * boot.get_logical_block_size()
                                < REQUIRED_ESP_SIZE))
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "the ESP partition must be at least 256 MiB in size",
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

    /// Loads existing logical volume data into memory, excluding encrypted volumes.
    pub fn initialize_volume_groups(&mut self) -> Result<(), DiskError> {
        let mut existing_devices: Vec<LogicalDevice> = Vec::new();

        for disk in &self.physical {
            let sector_size = disk.get_logical_block_size();
            for partition in disk.get_partitions().iter() {
                if let Some(ref lvm) = partition.lvm_vg {
                    // TODO: NLL
                    let push = match existing_devices.iter_mut().find(|d| &d.volume_group == lvm) {
                        Some(device) => {
                            device.add_sectors(partition.get_sectors());
                            false
                        }
                        None => true,
                    };

                    if push {
                        existing_devices.push(LogicalDevice::new(
                            lvm.clone(),
                            partition.encryption.clone(),
                            partition.get_sectors(),
                            sector_size,
                            false,
                        ));
                    }
                } else if let Some(ref vg) = partition.original_vg {
                    info!("found existing LVM device on {:?}", partition.get_device_path());
                    // TODO: NLL
                    let mut found = false;

                    if let Some(ref mut device) =
                        existing_devices.iter_mut().find(|d| d.volume_group.as_str() == vg.as_str())
                    {
                        device.add_sectors(partition.get_sectors());
                        found = true;
                    }

                    if !found {
                        existing_devices.push(LogicalDevice::new(
                            vg.clone(),
                            None,
                            partition.get_sectors(),
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
    pub fn commit_logical_partitions(&mut self) -> Result<(), DiskError> {
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
            for partition in
                device.file_system.as_mut().into_iter().chain(device.partitions.iter_mut())
            {
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

        // LUKS associations with LVM devices.
        let mut associations = Vec::new();

        // Now we will apply the logical layout.
        for (id, device) in self.logical.iter().enumerate() {
            // Only create the device if it does not exist.
            if !device.is_source {
                let volumes: Vec<(&Path, &Path)> = self.find_volume_paths(&device.volume_group);
                let mut device_path = None;

                if let Some(encryption) = device.encryption.as_ref() {
                    encryption.encrypt(volumes[0].1)?;
                    encryption.open(volumes[0].1)?;
                    encryption.create_physical_volume()?;
                    device_path =
                        Some(PathBuf::from(["/dev/mapper/", &encryption.physical_volume].concat()));

                    associations.push((volumes[0].1.to_path_buf(), id));
                }

                // Obtains an iterator which may produce one or more device paths.
                let volumes: Box<dyn Iterator<Item = &Path>> = match device_path.as_ref() {
                    // There will be only one volume, which we obtained from encryption.
                    Some(path) => Box::new(iter::once(path.as_path())),
                    // There may be more than one volume within a unencrypted LVM config.
                    None => Box::new(volumes.into_iter().map(|(_, part)| part)),
                };

                device.create_volume_group(volumes)?;
            }

            device.modify_partitions()?;
        }

        for (luks_parent, id) in associations {
            let mut logical = &mut self.logical[id];
            info!("associating {:?} with {:?}", logical.device_path, luks_parent);
            logical.luks_parent = Some(luks_parent);
        }

        // FS on LUKS
        for device in self.get_partitions_mut() {
            let format = device.flag_is_enabled(FORMAT);
            device.flag_disable(FORMAT);
            if let Some(encryption) = device.encryption.as_mut() {
                if encryption.filesystem == FileSystem::Lvm {
                    continue
                }

                if format {
                    encryption.encrypt(&device.device_path)?;
                    encryption.open(&device.device_path)?;

                    distinst_external_commands::mkfs(
                        &["/dev/mapper/", device.name.as_deref().unwrap()].concat(),
                        encryption.filesystem
                    ).unwrap();
                }
            }
        }

        Ok(())
    }
}

impl IntoIterator for Disks {
    type IntoIter = ::std::vec::IntoIter<Disk>;
    type Item = Disk;

    fn into_iter(self) -> Self::IntoIter { self.physical.into_iter() }
}

impl FromIterator<Disk> for Disks {
    fn from_iter<I: IntoIterator<Item = Disk>>(iter: I) -> Self {
        // TODO: Also collect LVM Devices
        Disks { physical: iter.into_iter().collect(), logical: Vec::new() }
    }
}

fn find_device_path_of_mount<P: AsRef<Path>>(path: P) -> io::Result<PathBuf> {
    let path = path.as_ref();
    for mount in MountIter::new()? {
        let mount = mount?;
        if mount.dest == path {
            return Ok(mount.source);
        }
    }

    Err(io::Error::new(io::ErrorKind::NotFound, "mount not found"))
}
