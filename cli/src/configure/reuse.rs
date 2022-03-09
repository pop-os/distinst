use super::*;
use errors::DistinstError;

pub(crate) fn reused(disks: &mut Disks, parts: Option<Values>) -> Result<(), DistinstError> {
    eprintln!("distinst: configuring reused partitions");
    if let Some(parts) = parts {
        for part in parts {
            let values: Vec<&str> = part.split(':').collect();
            if values.len() < 3 || values.len() > 5 {
                return Err(DistinstError::ReusedArgs);
            }

            let (block_dev, part_id, fs) = (
                values[0],
                values[1]
                    .parse::<u32>()
                    .map(|id| id as i32)
                    .map_err(|_| DistinstError::ArgNaN { arg: values[1].into() })?,
                match values[2] {
                    "reuse" => None,
                    fs => Some(parse_fs(fs)?),
                },
            );

            let (mut key, mut mount, mut flags) = (None, None, None);

            for value in values.iter().skip(3) {
                if value.starts_with("mount=") {
                    mount = Some(Path::new(&value[6..]));
                } else if value.starts_with("flags=") {
                    flags = Some(parse_flags(&value[6..]));
                } else if value.starts_with("keyid=") {
                    key = Some(String::from(&value[6..]));
                } else {
                    return Err(DistinstError::InvalidField { field: (*value).into() });
                }
            }

            let disk = find_disk_mut(disks, block_dev)?;
            let partition = find_partition_mut(disk, part_id)?;

            if let Some(keyid) = key {
                match mount {
                    Some(mount) => {
                        partition.associate_keyfile(keyid);
                        partition.set_mount(mount.into());
                    }
                    None => {
                        return Err(DistinstError::NoMountPath);
                    }
                }
            } else if let Some(mount) = mount {
                partition.set_mount(Path::new(mount).to_path_buf());
            }

            if let Some(fs) = fs {
                let fs = match fs {
                    PartType::Fs(fs) => fs,
                    PartType::Luks(encryption) => {
                        partition.set_encryption(encryption);
                        Some(FileSystem::Luks)
                    }
                    PartType::Lvm(volume_group, encryption) => {
                        partition.set_volume_group(volume_group);
                        if let Some(encryption) = encryption {
                            partition.set_encryption(encryption);
                        }
                        Some(FileSystem::Lvm)
                    }
                };

                if let Some(fs) = fs {
                    partition.format_with(fs);
                }
            }

            if let Some(flags) = flags {
                partition.flags = flags;
            }
        }
    }

    Ok(())
}
