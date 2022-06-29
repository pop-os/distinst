use self::FileSystem::*;
use super::bitflags::FileSystemSupport;
use disk_types::{BlockDeviceExt, FileSystem, PartitionExt};
use crate::disks::{Disks};
use crate::errors::IntoIoResult;
use crate::external::generate_unique_id;
use fstab_generate::BlockInfo;
use crate::misc::hasher;
use partition_identity::PartitionID;
use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    io,
};

pub trait InstallerDiskOps: Sync {
    /// Generates the crypttab and fstab files in memory.
    fn generate_fstabs(&self) -> (OsString, OsString);

    /// Find the root partition's block info from this disks object.
    fn get_block_info_of(&self, mount: &str) -> io::Result<BlockInfo>;

    /// Reports file systems that need to be supported in the install.
    fn get_support_flags(&self) -> FileSystemSupport;

    /// Gives a rootflags option if it's required by the installation
    fn rootflags(&self) -> Option<String>;
}

impl InstallerDiskOps for Disks {
    /// Generates the crypttab and fstab files in memory.
    fn generate_fstabs(&self) -> (OsString, OsString) {
        let &Disks { ref logical, ref physical, .. } = self;

        info!("generating /etc/crypttab & /etc/fstab in memory");
        let mut crypttab = OsString::with_capacity(1024);
        let mut fstab = String::with_capacity(1024);

        let partitions = physical
            .iter()
            .flat_map(|x| {
                x.file_system
                    .as_ref()
                    .into_iter()
                    .chain(x.partitions.iter())
                    .map(|p| (true, &None, p))
            })
            .chain(logical.iter().flat_map(|x| {
                let luks_parent = &x.luks_parent;
                let is_unencrypted: bool = x.encryption.is_none();
                x.file_system
                    .as_ref()
                    .into_iter()
                    .chain(x.partitions.iter())
                    .map(move |p| (is_unencrypted, luks_parent, p))
            }));

        let mut swap_uuids: Vec<u64> = Vec::new();
        let mut crypt_ids: Vec<u64> = Vec::new();

        for (is_unencrypted, luks_parent, partition) in partitions {
            if let Some(enc) = partition.encryption.as_ref() {
                let password: Cow<'static, OsStr> =
                    match (enc.password.is_some(), enc.keydata.as_ref()) {
                        (true, None) => Cow::Borrowed(OsStr::new("none")),
                        (false, None) => Cow::Borrowed(OsStr::new("/dev/urandom")),
                        (true, Some(_key)) => unimplemented!(),
                        (false, Some(&(_, ref key))) => {
                            let path = key
                                .clone()
                                .expect("should have been populated")
                                .1
                                .join(&enc.physical_volume);
                            Cow::Owned(path.into_os_string())
                        }
                    };

                let ppath = partition.get_device_path();

                if partition.lvm_vg.is_some() {
                    let luks_path = luks_parent.as_ref().map_or(ppath, |x| &x);
                    for logical in logical {
                        if let Some(ref parent) = logical.luks_parent {
                            if parent == ppath {
                                if logical.partitions.iter().any(|p| p.target.is_some()) {
                                    match PartitionID::get_uuid(luks_path) {
                                        Some(uuid) => {
                                            let id = hasher(&enc.physical_volume);
                                            if !crypt_ids.contains(&id) {
                                                crypt_ids.push(id);

                                                crypttab.push(&enc.physical_volume);
                                                crypttab.push(" UUID=");
                                                crypttab.push(&uuid.id);
                                                crypttab.push(" ");
                                                crypttab.push(&password);
                                                crypttab.push(" luks\n");
                                            }
                                        }
                                        None => warn!(
                                            "unable to find UUID for {} -- skipping",
                                            ppath.display()
                                        ),
                                    }
                                }
                                break;
                            }
                        }
                    }
                }

                if let Some(mut blockinfo) = partition.get_block_info() {
                    use std::path::Path;

                    let child_id = PartitionID::get_uuid(&Path::new(&["/dev/mapper/", &enc.physical_volume].concat()));
                    let id = PartitionID::get_uuid(&partition.device_path);

                    match id.zip(child_id) {
                        Some((uuid, child_uuid)) => {
                            crypttab.push(&enc.physical_volume);
                            crypttab.push(" UUID=");
                            crypttab.push(&uuid.id);
                            crypttab.push(" ");
                            crypttab.push(&password);
                            crypttab.push(" luks\n");

                            blockinfo.uid = child_uuid;
                            blockinfo.fs = enc.filesystem.into();

                            if enc.filesystem == FileSystem::Btrfs {
                                for (target, subvol) in &partition.subvolumes {
                                    blockinfo.options = ["subvol=", &*subvol].concat().into();
                                    blockinfo.mount = Some(target.clone());
                                    blockinfo.write(&mut fstab);
                                }

                                continue
                            }
                        }
                        None => {
                            warn!(
                                "unable to find UUID for {} -- skipping",
                                ppath.display()
                            );
                            continue;
                        }
                    }

                    blockinfo.write(&mut fstab);
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
                                " /dev/urandom swap,plain,offset=1024,cipher=aes-xts-plain64,size=512\n",
                            );

                            fstab.push_str(
                                &["/dev/mapper/", &unique_id, "  none  swap  defaults  0  0\n"]
                                    .concat(),
                            );
                        }
                        None => warn!(
                            "unable to find UUID for {} -- skipping",
                            partition.get_device_path().display()
                        ),
                    }
                } else {
                    let path = partition.get_device_path().to_str().expect("device with non-UTF8 path");
                    fstab.push_str(path);
                    fstab.push_str("  none  swap  defaults  0  0\n");
                }
            } else if let Some(mut blockinfo) = partition.get_block_info() {
                if partition.filesystem == Some(FileSystem::Btrfs) {
                    for (target, subvol) in &partition.subvolumes {
                        blockinfo.options =  ["subvol=", &*subvol].concat().into();
                        blockinfo.mount = Some(target.clone());
                        blockinfo.write(&mut fstab);
                    }

                    continue
                }

                blockinfo.write(&mut fstab);
            }
        }

        info!("generated the following crypttab data:\n{}", crypttab.to_string_lossy(),);

        info!("generated the following fstab data:\n{}", fstab);

        crypttab.shrink_to_fit();
        fstab.shrink_to_fit();
        (crypttab, OsString::from(fstab))
    }

    fn get_block_info_of(&self, path: &str) -> io::Result<BlockInfo> {
        self.find_partition(path.as_ref())
            .and_then(|(_, part)| part.get_block_info())
            .into_io_result(|| format!("get_block_info_of: partition {} found", path))

    }

    fn get_support_flags(&self) -> FileSystemSupport {
        let mut flags = FileSystemSupport::empty();

        let mut check = |fs| {
            match fs {
                Btrfs => flags |= FileSystemSupport::BTRFS,
                Ext2 | Ext3 | Ext4 => flags |= FileSystemSupport::EXT4,
                F2fs => flags |= FileSystemSupport::F2FS,
                Fat16 | Fat32 => flags |= FileSystemSupport::FAT,
                Ntfs => flags |= FileSystemSupport::NTFS,
                Xfs => flags |= FileSystemSupport::XFS,
                Luks => flags |= FileSystemSupport::LUKS,
                Lvm => flags |= FileSystemSupport::LVM,
                _ => (),
            };
        };

        for partition in self.get_partitions() {
            if let Some(fs) = partition.filesystem {
                check(fs);
            }

            if let Some(fs) = partition.encryption.as_ref().map(|e| e.filesystem) {
                check(fs);
            }
        }

        flags
    }

    fn rootflags(&self) -> Option<String> {
        let root = std::path::Path::new("/");
        self.find_partition(root)
            .expect("no root partition")
            .1
            .subvolumes
            .get(root)
            .map(|subvol| ["rootflags=subvol=", &*subvol].concat())
    }
}
