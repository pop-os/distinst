mod builder;

pub use self::builder::PartitionBuilder;
use super::{
    super::{LuksEncryption, PartitionError},
    PVS,
};
pub use disk_types::{BlockDeviceExt, FileSystem, PartitionExt, PartitionType, SectorExt};
use crate::{external::{get_label, is_encrypted}};
use fstab_generate::BlockInfo;
use libparted::{Partition, PartitionFlag};
pub use os_detect::OS;
use partition_identity::PartitionIdentifiers;
use proc_mounts::{MountList, SwapList};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    str::FromStr,
};
use sys_mount::swapoff;

pub fn get_preferred_options(fs: FileSystem) -> &'static str {
    match fs {
        FileSystem::Fat16 | FileSystem::Fat32 => "umask=0077",
        FileSystem::Ext4 => "noatime,errors=remount-ro",
        FileSystem::Swap => "sw",
        _ => "defaults",
    }
}

// Defines that this partition exists in the source.
pub const SOURCE: u8 = 0b00_0001;
// Defines that this partition will be removed.
pub const REMOVE: u8 = 0b00_0010;
// Defines that this partition will be formatted.
pub const FORMAT: u8 = 0b00_0100;
// Defines that this partition is currently active.
pub const ACTIVE: u8 = 0b00_1000;
// Defines that this partition is currently busy.
pub const BUSY: u8 = 0b01_0000;
// Defines that this partition is currently swapped.
pub const SWAPPED: u8 = 0b10_0000;

/// Contains relevant information about a certain partition.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionInfo {
    pub bitflags:     u8,
    /// The partition number is the numeric value that follows the disk's device path.
    /// IE: _/dev/sda1_
    pub number:       i32,
    /// The physical order of the partition on the disk, as partition numbers may not be in order.
    pub ordering:     i32,
    /// The initial sector where the partition currently, or will, reside.
    pub start_sector: u64,
    /// The final sector where the partition currently, or will, reside.
    /// # Note
    /// The length of the partion can be calculated by substracting the `end_sector`
    /// from the `start_sector`, and multiplying that by the value of the disk's
    /// sector size.
    pub end_sector:   u64,
    /// Whether this partition is a primary or logical partition.
    pub part_type:    PartitionType,
    /// Whether there is a file system currently, or will be, on this partition.
    pub filesystem:   Option<FileSystem>,
    /// Specifies optional flags that should be applied to the partition, if
    /// not already set.
    pub flags:        Vec<PartitionFlag>,
    /// Specifies the name of the partition.
    pub name:         Option<String>,
    /// Contains the device path of the partition, which is the disk's device path plus
    /// the partition number.
    pub device_path:  PathBuf,
    /// Where this partition is mounted in the file system, if at all.
    pub mount_point:  Vec<PathBuf>,
    /// Where this partition will be mounted in the future
    pub target:       Option<PathBuf>,
    /// The pre-existing volume group assigned to this partition.
    pub original_vg:  Option<String>,
    /// The LVM volume group
    pub lvm_vg: Option<String>,
    pub encryption: Option<LuksEncryption>,
    /// If the partition is associated with a keyfile, this will name the key.
    pub key_id:       Option<String>,
    /// Possible identifiers for this partition.
    pub identifiers:  PartitionIdentifiers,
    /// Subvolumes found on this device
    pub subvolumes:   HashMap<PathBuf, String>
}

impl BlockDeviceExt for PartitionInfo {
    fn get_device_path(&self) -> &Path { &self.device_path }

    fn get_mount_point(&self) -> &[PathBuf] { self.mount_point.as_ref() }
}

impl PartitionExt for PartitionInfo {
    fn get_file_system(&self) -> Option<FileSystem> { self.filesystem }

    fn get_partition_flags(&self) -> &[PartitionFlag] { &self.flags }

    fn get_partition_label(&self) -> Option<&str> { self.name.as_deref() }

    fn get_partition_type(&self) -> PartitionType { self.part_type }

    fn get_sector_end(&self) -> u64 { self.end_sector }

    fn get_sector_start(&self) -> u64 { self.start_sector }

    fn encrypted_info(&self) -> Option<(PathBuf, FileSystem)> {
        self.encryption.as_ref()
            .map(|enc| {
                let path = PathBuf::from(["/dev/mapper/", &*enc.physical_volume].concat());
                (path, enc.filesystem)
            })
    }
}

impl SectorExt for PartitionInfo {
    fn get_sectors(&self) -> u64 {
        self.get_sector_end() - self.get_sector_start()
    }
}

impl PartitionInfo {
    pub fn new_from_ped(partition: &Partition) -> io::Result<Option<PartitionInfo>> {
        let device_path =
            partition.get_path().expect("unable to get path from ped partition").to_path_buf();
        info!("obtaining partition information from {}", device_path.display());

        let identifiers = PartitionIdentifiers::from_path(&device_path);

        let filesystem = partition.fs_type_name().and_then(|name| FileSystem::from_str(name).ok());

        Ok(Some(PartitionInfo {
            bitflags: SOURCE
                | if partition.is_active() { ACTIVE } else { 0 }
                | if partition.is_busy() { BUSY } else { 0 },
            part_type: match partition.type_get_name() {
                "primary" => PartitionType::Primary,
                "logical" => PartitionType::Logical,
                _ => return Ok(None),
            },
            mount_point: Vec::new(),
            target: None,
            filesystem,
            flags: get_flags(partition),
            number: partition.num(),
            ordering: -1,
            name: filesystem.and_then(|fs| get_label(&device_path, fs)),
            device_path,
            start_sector: partition.geom_start() as u64,
            end_sector: partition.geom_end() as u64,
            original_vg: None,
            lvm_vg: None,
            encryption: None,
            key_id: None,
            identifiers,
            subvolumes: HashMap::new(),
        }))
    }

    pub fn collect_extended_information(&mut self, mounts: &MountList, swaps: &SwapList) {
        let device_path = &self.device_path;
        let original_vg =
            unsafe { PVS.as_ref().unwrap().get(device_path).and_then(|vg| vg.as_ref().cloned()) };

        if let Some(ref vg) = original_vg.as_ref() {
            info!("partition belongs to volume group '{}'", vg);
        }

        if self.filesystem.is_none() {
            self.filesystem = if is_encrypted(device_path) {
                Some(FileSystem::Luks)
            } else if original_vg.is_some() {
                Some(FileSystem::Lvm)
            } else {
                None
            };
        }

        self.mount_point = mounts.0
            .iter()
            .filter(|mount| &mount.source == device_path)
            .map(|m| m.dest.clone())
            .collect();

        self.bitflags |= if swaps.get_swapped(device_path) { SWAPPED } else { 0 };
        self.original_vg = original_vg;
    }

    pub fn deactivate_if_swap(&mut self, swaps: &SwapList) -> Result<(), (PathBuf, io::Error)> {
        {
            let path = &self.get_device_path();
            if swaps.get_swapped(path) {
                swapoff(path).map_err(|why| (path.to_path_buf(), why))?;
            }
        }
        self.mount_point = Vec::new();
        self.flag_disable(SWAPPED);
        Ok(())
    }

    pub fn flag_is_enabled(&self, flag: u8) -> bool { self.bitflags & flag != 0 }

    pub fn flag_disable(&mut self, flag: u8) { self.bitflags &= 255 ^ flag; }

    /// Assigns the partition to a keyfile ID.
    pub fn associate_keyfile(&mut self, id: String) {
        self.key_id = Some(id);
        self.target = None;
    }

    // True if the partition contains an encrypted partition
    pub fn is_encrypted(&self) -> bool { is_encrypted(self.get_device_path()) }

    pub fn get_current_lvm_volume_group(&self) -> Option<&str> {
        self.original_vg.as_deref()
    }

    /// True if the compared partition has differing parameters from the source.
    pub fn requires_changes(&self, other: &PartitionInfo) -> bool {
        self.sectors_differ_from(other)
            || self.filesystem != other.filesystem
            || self.flags != other.flags
            || other.flag_is_enabled(FORMAT)
    }

    /// True if the compared partition is the same as the source.
    pub fn is_same_partition_as(&self, other: &PartitionInfo) -> bool {
        self.flag_is_enabled(SOURCE) && other.flag_is_enabled(SOURCE) && self.number == other.number
    }

    /// Defines a mount target for this partition.
    pub fn set_mount(&mut self, target: PathBuf) { self.target = Some(target); }

    /// Defines that the partition belongs to a given volume group.
    pub fn set_volume_group(&mut self, group: String) {
        self.lvm_vg = Some(group);
    }

    pub fn set_encryption(&mut self, encryption: LuksEncryption) {
        self.encryption = Some(encryption);
    }

    /// Shrinks the partition, if possible.
    ///
    /// The provided value will be truncated to the nearest mebibyte, and returned.
    pub fn shrink_to(&mut self, mut sectors: u64) -> Result<u64, PartitionError> {
        sectors -= sectors % (2 * 1024);
        if self.end_sector - self.start_sector < sectors {
            Err(PartitionError::ShrinkValueTooHigh)
        } else {
            self.end_sector = self.start_sector + sectors;
            eprintln!("shrinking to {} sectors", sectors);
            assert_eq!(0, (self.end_sector - self.start_sector) % (2 * 1024));
            Ok(sectors)
        }
    }

    /// Defines that a new file system will be applied to this partition.
    /// NOTE: this will also unset the partition's name.
    pub fn format_with(&mut self, fs: FileSystem) {
        self.bitflags |= FORMAT;
        self.filesystem = Some(fs);
        self.name = None;
    }

    /// Defines that a new file system will be applied to this partition.
    /// Unlike `format_with`, this will not remove the name.
    pub fn format_and_keep_name(&mut self, fs: FileSystem) {
        self.bitflags |= FORMAT;
        self.filesystem = Some(fs);
    }

    /// Returns true if this partition will be formatted.
    pub fn will_format(&self) -> bool { self.bitflags & FORMAT != 0 }

    /// Specifies to delete this partition from the partition table.
    pub fn remove(&mut self) { self.bitflags |= REMOVE; }

    /// Obtains bock information for the partition, if possible, for use with
    /// generating entries in "/etc/fstab".
    pub fn get_block_info(&self) -> Option<BlockInfo> {
        let fs = self.get_file_system()?;
        if fs != FileSystem::Swap && self.target.is_none() && self.subvolumes.is_empty() {
            return None;
        }

        Some(BlockInfo::new(
            BlockInfo::get_partition_id(&self.device_path, fs)?,
            fs,
            self.target.as_deref(),
            get_preferred_options(fs),
        ))
    }
}

const FLAGS: &[PartitionFlag] = &[
    PartitionFlag::PED_PARTITION_BOOT,
    PartitionFlag::PED_PARTITION_ROOT,
    PartitionFlag::PED_PARTITION_SWAP,
    PartitionFlag::PED_PARTITION_HIDDEN,
    PartitionFlag::PED_PARTITION_RAID,
    PartitionFlag::PED_PARTITION_LVM,
    PartitionFlag::PED_PARTITION_LBA,
    PartitionFlag::PED_PARTITION_HPSERVICE,
    PartitionFlag::PED_PARTITION_PALO,
    PartitionFlag::PED_PARTITION_PREP,
    PartitionFlag::PED_PARTITION_MSFT_RESERVED,
    PartitionFlag::PED_PARTITION_BIOS_GRUB,
    PartitionFlag::PED_PARTITION_APPLE_TV_RECOVERY,
    PartitionFlag::PED_PARTITION_DIAG,
    PartitionFlag::PED_PARTITION_LEGACY_BOOT,
    PartitionFlag::PED_PARTITION_MSFT_DATA,
    PartitionFlag::PED_PARTITION_IRST,
    PartitionFlag::PED_PARTITION_ESP,
];

fn get_flags(partition: &Partition) -> Vec<PartitionFlag> {
    FLAGS
        .iter()
        .filter(|&&f| partition.is_flag_available(f) && partition.get_flag(f))
        .cloned()
        .collect::<Vec<PartitionFlag>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn efi_partition() -> PartitionInfo {
        PartitionInfo {
            bitflags:     ACTIVE | BUSY | SOURCE,
            device_path:  Path::new("/dev/sdz1").to_path_buf(),
            flags:        vec![PartitionFlag::PED_PARTITION_ESP],
            mount_point:  vec!(Path::new("/boot/efi").to_path_buf()),
            target:       Some(Path::new("/boot/efi").to_path_buf()),
            start_sector: 2048,
            end_sector:   1026047,
            filesystem:   Some(FileSystem::Fat16),
            name:         None,
            number:       1,
            ordering:     1,
            part_type:    PartitionType::Primary,
            key_id:       None,
            original_vg:  None,
            lvm_vg: None,
            encryption: None,
            identifiers:  PartitionIdentifiers::default(),
            subvolumes: HashMap::new()
        }
    }

    fn root_partition() -> PartitionInfo {
        PartitionInfo {
            bitflags:     ACTIVE | BUSY | SOURCE,
            device_path:  Path::new("/dev/sdz2").to_path_buf(),
            flags:        vec![],
            mount_point:  vec!(Path::new("/").to_path_buf()),
            target:       Some(Path::new("/").to_path_buf()),
            start_sector: 1026048,
            end_sector:   420456447,
            filesystem:   Some(FileSystem::Btrfs),
            name:         Some("Pop!_OS".into()),
            number:       2,
            ordering:     2,
            part_type:    PartitionType::Primary,
            key_id:       None,
            original_vg:  None,
            lvm_vg: None,
            encryption: None,
            identifiers:  PartitionIdentifiers::default(),
            subvolumes:   HashMap::new(),
        }
    }

    fn luks_on_lvm_partition() -> PartitionInfo {
        PartitionInfo {
            bitflags:     ACTIVE | SOURCE,
            device_path:  Path::new("/dev/sdz3").to_path_buf(),
            flags:        vec![],
            mount_point:  vec![],
            target:       None,
            start_sector: 420456448,
            end_sector:   1936738303,
            filesystem:   Some(FileSystem::Luks),
            name:         None,
            number:       4,
            ordering:     4,
            part_type:    PartitionType::Primary,
            key_id:       None,
            original_vg:  None,
            identifiers:  PartitionIdentifiers::default(),
            lvm_vg: Some("LVM_GROUP".into()),
            encryption: Some(LuksEncryption {
                physical_volume: "LUKS_PV".into(),
                password:        Some("password".into()),
                keydata:         None,
                filesystem:      FileSystem::Lvm,
            }),
            subvolumes:   HashMap::new(),
        }
    }

    fn lvm_partition() -> PartitionInfo {
        PartitionInfo {
            bitflags:     ACTIVE | SOURCE,
            device_path:  Path::new("/dev/sdz3").to_path_buf(),
            flags:        vec![],
            mount_point:  vec![],
            target:       None,
            start_sector: 420456448,
            end_sector:   1936738303,
            filesystem:   Some(FileSystem::Lvm),
            name:         None,
            number:       4,
            ordering:     4,
            part_type:    PartitionType::Primary,
            key_id:       None,
            original_vg:  None,
            lvm_vg: Some("LVM_GROUP".into()),
            encryption:   None,
            identifiers:  PartitionIdentifiers::default(),
            subvolumes:   HashMap::new(),
        }
    }

    fn swap_partition() -> PartitionInfo {
        PartitionInfo {
            bitflags:     ACTIVE | SOURCE,
            device_path:  Path::new("/dev/sdz4").to_path_buf(),
            flags:        vec![],
            mount_point:  vec![],
            target:       None,
            start_sector: 1936738304,
            end_sector:   1953523711,
            filesystem:   Some(FileSystem::Swap),
            name:         None,
            number:       4,
            ordering:     4,
            part_type:    PartitionType::Primary,
            key_id:       None,
            original_vg:  None,
            lvm_vg: None,
            encryption: None,
            identifiers:  PartitionIdentifiers::default(),
            subvolumes:   HashMap::new(),
        }
    }

    #[test]
    fn partition_sectors() {
        assert_eq!(swap_partition().get_sectors(), 16785407);
        assert_eq!(root_partition().get_sectors(), 419430399);
        assert_eq!(efi_partition().get_sectors(), 1023999);
    }

    #[test]
    fn partition_is_esp_partition() {
        assert!(!root_partition().is_esp_partition());
        assert!(efi_partition().is_esp_partition());
    }

    #[test]
    fn partition_is_linux_compatible() {
        assert!(root_partition().is_linux_compatible());
        assert!(!swap_partition().is_linux_compatible());
        assert!(!efi_partition().is_linux_compatible());
        assert!(!luks_on_lvm_partition().is_linux_compatible());
        assert!(!lvm_partition().is_linux_compatible());
    }

    #[test]
    fn partition_requires_changes() {
        let root = root_partition();

        {
            let mut other = root_partition();
            assert!(!root.requires_changes(&other));
            other.start_sector = 0;
            assert!(root.requires_changes(&other));
        }

        {
            let mut other = root_partition();
            other.format_with(FileSystem::Btrfs);
            assert!(root.requires_changes(&other));
        }
    }

    #[test]
    fn partition_sectors_differ_from() {
        assert!(root_partition().sectors_differ_from(&efi_partition()));
        assert!(!root_partition().sectors_differ_from(&root_partition()));
    }

    #[test]
    fn partition_is_same_as() {
        let root = root_partition();
        let root_dup = root.clone();
        let efi = efi_partition();

        assert!(root.is_same_partition_as(&root_dup));
        assert!(!root.is_same_partition_as(&efi));
    }
}
