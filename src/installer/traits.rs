use disk_types::{BlockDeviceExt, FileSystem, PartitionExt};
use disks::{Disks, PartitionInfo};
use external::generate_unique_id;
use fstab_generate::BlockInfo;
use misc::hasher;
use partition_identity::PartitionID;
use self::FileSystem::*;
use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::io;
use super::bitflags::FileSystemSupport;

pub trait InstallerDiskOps: Sync {
    /// Generates the crypttab and fstab files in memory.
    fn generate_fstabs(&self) -> (OsString, OsString);

    /// Find the root partition's block info from this disks object.
    fn get_root_block_info(&self) -> io::Result<BlockInfo>;

    /// Reports file systems that need to be supported in the install.
    fn get_support_flags(&self) -> FileSystemSupport;
}

impl InstallerDiskOps for Disks {
    /// Generates the crypttab and fstab files in memory.
    fn generate_fstabs(&self) -> (OsString, OsString) {
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
                entry.write_entry(fstab);
            }
        }

        let mut swap_uuids: Vec<u64> = Vec::new();

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

                let path = luks_parent.as_ref().map_or(partition.get_device_path(), |x| &x);

                match PartitionID::get_uuid(path) {
                    Some(uuid) => {
                        crypttab.push(&enc.physical_volume);
                        crypttab.push(" UUID=");
                        crypttab.push(&uuid.id);
                        crypttab.push(" ");
                        crypttab.push(&password);
                        crypttab.push(" luks\n");
                        write_fstab(&mut fstab, &partition);
                    }
                    None => error!(
                        "unable to find UUID for {} -- skipping",
                        partition.get_device_path().display()
                    ),
                }
            } else if partition.is_swap() {
                if is_unencrypted {
                    match PartitionID::get_uuid(&partition.get_device_path()) {
                        Some(uuid) => {
                            let unique_id = generate_unique_id("cryptswap", &swap_uuids)
                                .unwrap_or_else(|_| "cryptswap".into());

                            swap_uuids.push(hasher(&unique_id));

                            crypttab.push(&unique_id);
                            crypttab.push(" UUID=");
                            crypttab.push(&uuid.id);
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
                            partition.get_device_path().display()
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

    fn get_root_block_info(&self) -> io::Result<BlockInfo> {
        self.get_partitions()
            .filter_map(|part| part.get_block_info())
            .find(|entry| entry.mount() == "/")
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::Other,
                "root partition not found",
            ))
    }

    fn get_support_flags(&self) -> FileSystemSupport {
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
}
