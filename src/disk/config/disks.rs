use super::{Disk, get_uuid};
use super::super::{Bootloader, DiskError, DiskExt, PartitionFlag, PartitionInfo};
use super::find_partition;
use super::super::external::{cryptsetup_close, deactivate_volumes, lvs, pvremove, pvs, vgremove};
use super::super::lvm::{self, LvmDevice};
use super::super::mount::umount;
use super::super::mounts::Mounts;
use libparted::{Device, DeviceType};

use itertools::Itertools;
use std::collections::HashSet;
use std::ffi::OsString;
use std::io;
use std::iter::{self, FromIterator};
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
