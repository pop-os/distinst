use super::*;
use distinst::disks::DiskExt;
use errors::DistinstError;

pub(crate) fn new(disks: &mut Disks, parts: Option<Values>) -> Result<(), DistinstError> {
    eprintln!("distinst: configuring new partitions");
    if let Some(parts) = parts {
        for part in parts {
            let values: Vec<&str> = part.split(':').collect();
            if values.len() < 5 || values.len() > 7 {
                return Err(DistinstError::NewArgs);
            }

            let (block, kind, start, end, fs) = (
                values[0],
                parse_part_type(values[1])?,
                parse_sector(values[2])?,
                parse_sector(values[3])?,
                parse_fs(values[4])?,
            );

            let (mut key, mut mount, mut flags) = (None, None, None);

            for value in values.iter().skip(5) {
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

            let disk = find_disk_mut(disks, block)?;

            let start = disk.get_sector(start);
            let end = disk.get_sector(end);
            let mut builder = match fs {
                PartType::Luks(encryption) => {
                    PartitionBuilder::new(start, end, FileSystem::Luks)
                        .partition_type(kind)
                        .encryption(encryption)
                }
                PartType::Lvm(volume_group, encryption) => {
                    let mut builder = PartitionBuilder::new(start, end, FileSystem::Lvm)
                        .partition_type(kind)
                        .logical_volume(volume_group);

                    if let Some(params) = encryption {
                        builder = builder.encryption(params);
                    }

                    builder
                }
                PartType::Fs(fs) => PartitionBuilder::new(start, end, fs).partition_type(kind),
            };

            if let Some(flags) = flags {
                builder = builder.flags(flags);
            }

            if let Some(keyid) = key {
                match mount {
                    Some(mount) => {
                        builder = builder.associate_keyfile(keyid).mount(mount.into());
                    }
                    None => {
                        return Err(DistinstError::NoMountPath);
                    }
                }
            } else if let Some(mount) = mount {
                builder = builder.mount(mount.into());
            }

            disk.add_partition(builder)?;
        }
    }

    Ok(())
}

fn parse_part_type(table: &str) -> Result<PartitionType, DistinstError> {
    match table {
        "primary" => Ok(PartitionType::Primary),
        "logical" => Ok(PartitionType::Logical),
        "extended" => Ok(PartitionType::Extended),
        _ => Err(DistinstError::InvalidPartitionType),
    }
}
