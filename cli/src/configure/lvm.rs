use super::*;
use distinst::disks::DiskExt;
use errors::DistinstError;

pub(crate) fn lvm(
    disks: &mut Disks,
    logical: Option<Values>,
    modify: Option<Values>,
    remove: Option<Values>,
    remove_all: bool,
) -> Result<(), DistinstError> {
    eprintln!("distinst: configuring lvm / luks partitions");
    if remove_all {
        for device in disks.get_logical_devices_mut() {
            device.clear_partitions();
        }
    } else if let Some(remove) = remove {
        for value in remove {
            let values: Vec<&str> = value.split(':').collect();
            if values.len() != 2 {
                return Err(DistinstError::LogicalRemoveArgs);
            }

            let (group, volume) = (values[0], values[1]);
            let device = disks
                .get_logical_device_mut(group)
                .ok_or(DistinstError::LogicalDeviceNotFound { group: group.into() })?;

            device.remove_partition(volume)?;
        }
    }

    if let Some(modify) = modify {
        for value in modify {
            let values: Vec<&str> = value.split(':').collect();
            if values.len() < 3 {
                return Err(DistinstError::ModifyArgs);
            }

            let (group, volume) = (values[0], values[1]);
            let (mut fs, mut mount) = (None, None);

            for field in values.iter().skip(2) {
                if field.starts_with("fs=") {
                    fs = Some(parse_fs(&field[3..])?)
                } else if field.starts_with("mount=") {
                    mount = Some(&field[6..]);
                } else {
                    unimplemented!()
                }
            }

            let device = disks
                .get_logical_device_mut(group)
                .ok_or(DistinstError::LogicalDeviceNotFound { group: group.into() })?;

            let partition = device.get_partition_mut(volume).ok_or(
                DistinstError::LogicalPartitionNotFound {
                    group:  group.into(),
                    volume: volume.into(),
                },
            )?;

            if let Some(fs) = fs {
                let fs = match fs {
                    PartType::Fs(fs) => fs,
                    PartType::Luks(encryption) => {
                        partition.set_encryption(encryption);
                        Some(FileSystem::Luks)
                    }
                    PartType::Lvm(volume_group, encryption) => {
                        partition.set_volume_group(volume_group);
                        if let Some(params) = encryption {
                            partition.set_encryption(params);
                        }
                        Some(FileSystem::Lvm)
                    }
                };

                if let Some(fs) = fs {
                    partition.format_and_keep_name(fs);
                }
            }

            if let Some(mount) = mount {
                partition.set_mount(PathBuf::from(mount.to_owned()));
            }
        }
    }

    if let Some(logical) = logical {
        parse_logical(logical, |args| match disks.get_logical_device_mut(&args.group) {
            Some(lvm_device) => {
                let start = lvm_device.get_last_sector();
                let end = start + lvm_device.get_sector(args.size);
                let mut builder =
                    PartitionBuilder::new(start, end, args.fs).name(args.name.clone());

                if let Some(mount) = args.mount.as_ref() {
                    builder = builder.mount(mount.clone());
                }

                if let Some(flags) = args.flags.as_ref() {
                    builder = builder.flags(flags.clone());
                }

                lvm_device
                    .add_partition(builder)
                    .map_err(|why| DistinstError::LvmPartitionAdd { why })
            }
            None => Err(DistinstError::NoVolumeGroupAssociated { group: args.group }),
        })?;
    }

    Ok(())
}

// Defines a new partition to assign to a volume group
struct LogicalArgs {
    // The group to create a partition on
    group: String,
    // The name of the partition
    name:  String,
    // The length of the partition
    size:  Sector,
    // The filesystem to assign to this partition
    fs:    Option<FileSystem>,
    // Where to mount this partition
    mount: Option<PathBuf>,
    // The partition flags to assign
    flags: Option<Vec<PartitionFlag>>,
}

fn parse_logical<F: FnMut(LogicalArgs) -> Result<(), DistinstError>>(
    values: Values,
    mut action: F,
) -> Result<(), DistinstError> {
    for value in values {
        let values: Vec<&str> = value.split(':').collect();
        if values.len() < 4 || values.len() > 6 {
            return Err(DistinstError::LogicalArgs);
        }

        let (mut mount, mut flags) = (None, None);

        for arg in values.iter().skip(4) {
            if arg.starts_with("mount=") {
                let mountval = &arg[6..];
                if mountval.is_empty() {
                    return Err(DistinstError::EmptyMount);
                }

                mount = Some(Path::new(mountval).to_path_buf());
            } else if arg.starts_with("flags=") {
                let flagval = &arg[6..];
                if flagval.is_empty() {
                    return Err(DistinstError::EmptyMount);
                }

                flags = Some(parse_flags(flagval));
            } else {
                return Err(DistinstError::InvalidField { field: (*arg).into() });
            }
        }

        action(LogicalArgs {
            group: values[0].into(),
            name: values[1].into(),
            size: parse_sector(values[2])?,
            fs: match parse_fs(values[3])? {
                PartType::Fs(fs) => fs,
                _ => {
                    unimplemented!("LUKS on LVM is unsupported");
                }
            },
            mount,
            flags,
        })?;
    }

    Ok(())
}
