use super::find_partition;
use super::partitions::{FORMAT, REMOVE, SOURCE};
use super::super::external::{blkid_partition, cryptsetup_close, cryptsetup_open, lvs, pvremove, pvs, vgdeactivate, vgremove};
use super::super::lvm::{self, LvmDevice};
use super::super::mount::{self, swapoff, umount};
use super::super::mounts::Mounts;
use super::super::swaps::Swaps;
use super::super::{
    Bootloader, DecryptionError, DiskError, DiskExt, FileSystemType, PartitionFlag, PartitionInfo,
    PartitionType,
};
use super::{get_size, get_uuid, Disk, LvmEncryption};
use libparted::{Device, DeviceType};

use itertools::Itertools;
use std::collections::HashSet;
use std::ffi::OsString;
use std::io;
use std::iter::{self, FromIterator};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::str;

/// A configuration of disks, both physical and logical.
pub struct Disks {
    pub(crate) physical: Vec<Disk>,
    pub(crate) logical:  Vec<LvmDevice>,
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

    /// Returns a slice of physical disks stored within the configuration.
    pub fn get_physical_devices(&self) -> &[Disk] { &self.physical }

    /// Returns a mutable slice of physical disks stored within the
    /// configuration.
    pub fn get_physical_devices_mut(&mut self) -> &mut [Disk] { &mut self.physical }

    pub fn get_logical_device(&self, group: &str) -> Option<&LvmDevice> {
        self.logical.iter().find(|d| &d.volume_group == group)
    }

    pub fn get_logical_device_mut(&mut self, group: &str) -> Option<&mut LvmDevice> {
        self.logical.iter_mut().find(|d| &d.volume_group == group)
    }

    /// Returns a slice of logical disks stored within the configuration.
    pub fn get_logical_devices(&self) -> &[LvmDevice] { &self.logical }

    /// Returns a mutable slice of logical disks stored within the
    /// configuration.
    pub fn get_logical_devices_mut(&mut self) -> &mut [LvmDevice] { &mut self.logical }

    /// Returns a list of device paths which will be modified by this
    /// configuration.
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

    /// Deactivates all device maps associated with the inner disks/partitions
    /// to be modified.
    pub fn deactivate_device_maps(&self) -> Result<(), DiskError> {
        let mounts = Mounts::new().unwrap();
        let swaps = Swaps::new().unwrap();
        let umount = move |vg: &str| -> Result<(), DiskError> {
            for lv in lvs(vg).map_err(|why| DiskError::ExternalCommand { why })? {
                if let Some(mount) = mounts.get_mount_point(&lv) {
                    info!(
                        "libdistinst: unmounting logical volume mounted at {}",
                        mount.display()
                    );
                    umount(&mount, false).map_err(|why| DiskError::Unmount { why })?;
                } else if let Ok(lv) = lv.canonicalize() {
                    if swaps.get_swapped(&lv) {
                        swapoff(&lv).map_err(|why| DiskError::Unmount { why })?;
                    }
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
                    vgdeactivate(vg)
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
                umount(vg)
                    .and_then(|_| vgremove(vg).map_err(|why| DiskError::ExternalCommand { why }))?;
            }
        }

        Ok(())
    }

    /// Attempts to decrypt the specified partition.
    ///
    /// If successful, the new device will be added as a logical disk.
    /// At the moment, only LVM on LUKS configurations are supported here.
    /// LUKS on LUKS, or Something on LUKS, will simply error.
    pub fn decrypt_partition(
        &mut self,
        path: &Path,
        enc: LvmEncryption,
    ) -> Result<(), DecryptionError> {
        // An intermediary value that can avoid the borrowck issue.
        let mut new_device = None;

        // Attempt to find the device in the configuration.
        for device in &mut self.physical {
            for partition in &mut device.partitions {
                if &partition.device_path == path {
                    // Attempt to decrypt the device.
                    cryptsetup_open(path, &enc).map_err(|why| DecryptionError::Open {
                        device: path.to_path_buf(),
                        why,
                    })?;

                    // Determine which VG the newly-decrypted device belongs to.
                    let pv = &PathBuf::from(["/dev/mapper/", &enc.physical_volume].concat());
                    match pvs().unwrap().remove(pv) {
                        Some(Some(vg)) => {
                            // Set values in the device's partition.
                            partition.volume_group = Some((vg.clone(), Some(enc.clone())));

                            // Create a new LvmDevice structure.
                            new_device = Some(LvmDevice::new(
                                vg,
                                Some(enc.clone()),
                                partition.sectors(),
                                device.sector_size,
                                true,
                            ));

                            break;
                        }
                        _ => {
                            // Attempt to close the device as we've failed to find a VG.
                            let _ = cryptsetup_close(pv);

                            // NOTE: Should we handle this in some way?
                            return Err(DecryptionError::DecryptedLacksVG {
                                device: path.to_path_buf(),
                            });
                        }
                    }
                }
            }
        }

        match new_device {
            // Add the new LVM device to the disk configuration
            Some(device) => {
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
        let mounts = Mounts::new().unwrap();
        for device in self.get_physical_devices() {
            for mount in mounts.starts_with(device.get_device_path().as_os_str().as_bytes()) {
                info!(
                    "libdistinst: unmounting device mounted at {}",
                    mount.display()
                );
                mount::umount(&mount, false).map_err(|why| DiskError::Unmount { why })?;
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
                | DeviceType::PED_DEVICE_FILE
                | DeviceType::PED_DEVICE_DM => continue,
                _ => disks.add(Disk::new(&mut device)?),
            }
        }

        // TODO: Also collect LVM devices
        Ok(disks)
    }

    /// Returns an immutable reference to the disk specified by its path, if it
    /// exists.
    pub fn find_disk<P: AsRef<Path>>(&self, path: P) -> Option<&Disk> {
        self.physical
            .iter()
            .find(|disk| &disk.device_path == path.as_ref())
    }

    /// Returns a mutable reference to the disk specified by its path, if it
    /// exists.
    pub fn find_disk_mut<P: AsRef<Path>>(&mut self, path: P) -> Option<&mut Disk> {
        self.physical
            .iter_mut()
            .find(|disk| &disk.device_path == path.as_ref())
    }

    /// Returns an immutable reference to the disk specified by its path, if it
    /// exists.
    pub fn find_logical_disk(&self, group: &str) -> Option<&LvmDevice> {
        self.logical
            .iter()
            .find(|device| &device.volume_group == group)
    }

    /// Returns a mutable reference to the disk specified by its path, if it
    /// exists.
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
                    .or(partition.original_vg.as_ref());

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

    /// Maps key paths to their keyfile IDs
    fn resolve_keyfile_paths(&mut self) -> Result<(), DiskError> {
        let mut temp: Vec<(String, Option<(PathBuf, PathBuf)>)> = Vec::new();

        'outer: for logical_device in &mut self.logical {
            if let Some(ref mut encryption) = logical_device.encryption {
                if let Some((ref key_id, ref mut paths)) = encryption.keydata {
                    let partitions = self.physical.iter().flat_map(|p| p.partitions.iter());
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

        // TODO: Verify that encrypted partitions with a keyfile ID have one
        // partition match with the same keyfile ID.

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
                        (true, Some(_key)) => unimplemented!(),
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
                        .find(|d| &d.volume_group == &lvm.0)
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

        let mut start_sector = 0;
        for device in &mut existing_devices {
            if !device.is_source {
                continue;
            }
            if let Ok(logical_paths) = lvs(&device.volume_group) {
                for path in logical_paths {
                    let length = get_size(&path).unwrap_or(0);
                    let partition = PartitionInfo {
                        bitflags: SOURCE,
                        // This value doesn't matter to a logical volume.
                        number: -1,
                        start_sector,
                        end_sector: start_sector + length,
                        part_type: PartitionType::Primary,
                        // TODO: Figure out flags?
                        flags: vec![],
                        filesystem: blkid_partition(&path),
                        name: {
                            let dev = path.file_name().unwrap().to_str().unwrap();
                            let value = dev.find('-').map_or(0, |v| v + 1);
                            Some(dev.split_at(value).1.into())
                        },
                        device_path: path,
                        mount_point: None,
                        target: None,
                        original_vg: None,
                        // TODO: Check if this partition is assigned to a VG?
                        volume_group: None,
                        key_id: None,
                    };

                    start_sector += length + 1;
                    device.partitions.push(partition);
                }
            }
        }

        self.logical = existing_devices;

        Ok(())
    }

    pub fn remove_logical_device(&mut self, volume: &str) {
        let mut remove_id = None;
        for (id, device) in self.logical.iter_mut().enumerate() {
            if &device.volume_group == volume {
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
            for partition in &mut device.partitions {
                // ... unless it is populated, due to existing beforehand.
                if partition.flag_is_enabled(SOURCE) {
                    continue;
                }
                let label = partition.name.as_ref().unwrap();
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
