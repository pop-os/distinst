extern crate clap;
extern crate distinst;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate libc;
extern crate pbr;

use clap::{App, Arg, ArgMatches, Values};
use distinst::{
    Config, Disk, DiskError, DiskExt, Disks, FileSystemType, Installer, LvmEncryption,
    PartitionBuilder, PartitionFlag, PartitionInfo, PartitionTable, PartitionType, Sector, Step,
    KILL_SWITCH, PARTITIONING_TEST, FORCE_BOOTLOADER
};

use pbr::ProgressBar;

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::rc::Rc;
use std::sync::atomic::Ordering;

#[derive(Debug, Fail)]
enum DistinstError {
    #[fail(display = "disk error: {}", why)]
    Disk { why: DiskError },
    #[fail(display = "table argument requires two values")]
    TableArgs,
    #[fail(display = "'{}' is not a valid table. Must be either 'gpt' or 'msdos'.", table)]
    InvalidTable { table: String },
    #[fail(display = "partition type must be either 'primary' or 'logical'")]
    InvalidPartitionType,
    #[fail(display = "decryption argument requires four values")]
    DecryptArgs,
    #[fail(display = "disk at '{}' could not be found", disk)]
    DiskNotFound { disk: String },
    #[fail(display = "no block argument provided")]
    NoBlockArg,
    #[fail(display = "argument '{}' is not a number", arg)]
    ArgNaN { arg: String },
    #[fail(display = "partition '{}' was not found", partition)]
    PartitionNotFound { partition: i32 },
    #[fail(display = "four arguments must be supplied to the move operation")]
    MoveArgs,
    #[fail(display = "provided sector value, '{}', was invalid", value)]
    InvalidSectorValue { value: String },
    #[fail(display = "no physical volume was defined in file system field")]
    NoPhysicalVolume,
    #[fail(display = "no volume group was defined in file system field")]
    NoVolumeGroup,
    #[fail(display = "provided password was empty")]
    EmptyPassword,
    #[fail(display = "provided key value was empty")]
    EmptyKeyValue,
    #[fail(display = "invalid field: {}", field)]
    InvalidField { field: String },
    #[fail(display = "provided file system, '{}', was invalid", fs)]
    InvalidFileSystem { fs: String },
    #[fail(display = "no logical device named '{}' found", group)]
    LogicalDeviceNotFound { group: String },
    #[fail(display = "'{}' was not found on '{}'", volume, group)]
    LogicalPartitionNotFound { group:  String, volume: String },
    #[fail(display = "invalid number of arguments supplied to --logical-modify")]
    ModifyArgs,
    #[fail(display = "could not find volume group associated with '{}'", group)]
    NoVolumeGroupAssociated { group: String },
    #[fail(display = "invalid number of arguments supplied to --use")]
    ReusedArgs,
    #[fail(display = "invalid number of arguments supplied to --new")]
    NewArgs,
    #[fail(display = "invalid number of arguments supplied to --logical")]
    LogicalArgs,
    #[fail(display = "invalid number of arguments supplied to --logical-remove")]
    LogicalRemoveArgs,
    #[fail(display = "mount path must be specified with key")]
    NoMountPath,
    #[fail(display = "mount value is empty")]
    EmptyMount,
    #[fail(display = "unable to add partition to lvm device: {}", why)]
    LvmPartitionAdd { why: DiskError },
    #[fail(display = "unable to initialize volume groups: {}", why)]
    InitializeVolumes { why: DiskError },
}

impl From<DiskError> for DistinstError {
    fn from(why: DiskError) -> DistinstError { DistinstError::Disk { why } }
}

fn main() {
    let matches = App::new("distinst")
        .arg(
            Arg::with_name("squashfs")
                .short("s")
                .long("squashfs")
                .help("define the squashfs image which will be installed")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("hostname")
                .short("h")
                .long("hostname")
                .help("define the hostname that the new system will have")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("keyboard")
                .short("k")
                .long("keyboard")
                .help("define the keyboard configuration to use")
                .takes_value(true)
                .min_values(1)
                .max_values(3)
                .default_value("us"),
        )
        .arg(
            Arg::with_name("lang")
                .short("l")
                .long("lang")
                .help("define the locale that the new system will use")
                .takes_value(true)
                .default_value("en_US.UTF-8"),
        )
        .arg(
            Arg::with_name("remove")
                .short("r")
                .long("remove")
                .help("defines the manifest file that contains the packages to remove post-install")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("disk")
                .short("b")
                .long("block")
                .help("defines a disk that will be manipulated in the installation process")
                .takes_value(true)
                .multiple(true)
                .required(true),
        )
        .arg(
            Arg::with_name("table")
                .short("t")
                .long("new-table")
                .help(
                    "defines a new partition table to apply to the disk, clobbering it in the \
                     process",
                )
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("new")
                .short("n")
                .long("new")
                .help("defines a new partition that will be created on the disk")
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("use")
                .short("u")
                .long("use")
                .help("defines to reuse an existing partition on the disk")
                .takes_value(true)
                .multiple(true),
        )
        .arg(
            Arg::with_name("test")
                .long("test")
                .help("simply test whether the provided arguments pass the partitioning stage"),
        )
        .arg(
            Arg::with_name("force-bios")
                .long("force-bios")
                .help("performs a BIOS installation even if the running system is EFI")
        )
        .arg(
            Arg::with_name("force-efi")
                .long("force-efi")
                .help("performs an EFI installation even if the running system is BIOS")
        )
        .arg(
            Arg::with_name("delete")
                .short("d")
                .long("delete")
                .help("defines to delete the specified partitions")
                .takes_value(true)
                .multiple(true),
        )
        .arg(
            Arg::with_name("move")
                .short("m")
                .long("move")
                .help("defines to move and/or resize an existing partition")
                .takes_value(true)
                .multiple(true),
        )
        .arg(
            Arg::with_name("logical")
                .long("logical")
                .help("creates a partition on a LVM volume group")
                .takes_value(true)
                .multiple(true),
        )
        .arg(
            Arg::with_name("logical-modify")
                .long("logical-modify")
                .help("modifies an existing LVM volume group")
                .takes_value(true)
                .multiple(true),
        )
        .arg(
            Arg::with_name("logical-remove")
                .long("logical-remove")
                .help("removes an existing LVM logical volume")
                .takes_value(true)
                .multiple(true),
        )
        .arg(
            Arg::with_name("logical-remove-all")
                .long("logical-remove-all")
                .help("TODO")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("decrypt")
                .long("decrypt")
                .help("decrypts an existing LUKS partition")
                .takes_value(true)
                .multiple(true),
        )
        .get_matches();

    if let Err(err) = distinst::log(|_level, message| {
        println!("{}", message);
    }) {
        eprintln!("Failed to initialize logging: {}", err);
    }

    let squashfs = matches.value_of("squashfs").unwrap();
    let hostname = matches.value_of("hostname").unwrap();
    let mut keyboard = matches.values_of("keyboard").unwrap();
    let lang = matches.value_of("lang").unwrap();
    let remove = matches.value_of("remove").unwrap();

    let pb_opt: Rc<RefCell<Option<ProgressBar<io::Stdout>>>> = Rc::new(RefCell::new(None));

    let res = {
        let mut installer = Installer::default();

        {
            let pb_opt = pb_opt.clone();
            installer.on_error(move |error| {
                if let Some(mut pb) = pb_opt.borrow_mut().take() {
                    pb.finish_println("");
                }

                eprintln!("Error: {:?}", error);
            });
        }

        {
            let pb_opt = pb_opt.clone();
            let mut step_opt = None;
            installer.on_status(move |status| {
                if step_opt != Some(status.step) {
                    if let Some(mut pb) = pb_opt.borrow_mut().take() {
                        pb.finish_println("");
                    }

                    step_opt = Some(status.step);

                    let mut pb = ProgressBar::new(100);
                    pb.show_speed = false;
                    pb.show_counter = false;
                    pb.message(match status.step {
                        Step::Init => "Initializing",
                        Step::Partition => "Partitioning disk ",
                        Step::Extract => "Extracting filesystem ",
                        Step::Configure => "Configuring installation",
                        Step::Bootloader => "Installing bootloader ",
                    });
                    *pb_opt.borrow_mut() = Some(pb);
                }

                if let Some(ref mut pb) = *pb_opt.borrow_mut() {
                    pb.set(status.percent as u64);
                }
            });
        }

        let disks = match configure_disks(&matches) {
            Ok(disks) => disks,
            Err(why) => {
                eprintln!("distinst: {}", why);
                exit(1);
            }
        };

        configure_signal_handling();

        let testing = matches.occurrences_of("test") != 0;
        if testing {
            PARTITIONING_TEST.store(true, Ordering::SeqCst);
        }

        if matches.occurrences_of("force-bios") != 0 {
            FORCE_BOOTLOADER.store(1, Ordering::SeqCst);
        } else if matches.occurrences_of("force-efi") != 0 {
            FORCE_BOOTLOADER.store(2, Ordering::SeqCst);
        }

        fn take_optional_string(argument: Option<&str>) -> Option<String> {
            argument
                .map(String::from)
                .and_then(|x| if x.is_empty() { None } else { Some(x) })
        }

        installer.install(
            disks,
            &Config {
                flags:            if testing {
                    0
                } else {
                    distinst::MODIFY_BOOT_ORDER | distinst::INSTALL_HARDWARE_SUPPORT
                },
                hostname:         hostname.into(),
                keyboard_layout:  keyboard.next().map(String::from).unwrap(),
                keyboard_model:   take_optional_string(keyboard.next()),
                keyboard_variant: take_optional_string(keyboard.next()),
                old_root:         None,
                lang:             lang.into(),
                remove:           remove.into(),
                squashfs:         squashfs.into(),
            },
        )
    };

    if let Some(mut pb) = pb_opt.borrow_mut().take() {
        pb.finish_println("");
    }

    let status = match res {
        Ok(()) => {
            println!("install was successful");
            0
        }
        Err(err) => {
            println!("install failed: {}", err);
            1
        }
    };

    exit(status);
}

fn configure_signal_handling() {
    extern "C" fn handler(signal: i32) {
        match signal {
            libc::SIGINT => KILL_SWITCH.store(true, Ordering::SeqCst),
            _ => unreachable!(),
        }
    }

    if unsafe { libc::signal(libc::SIGINT, handler as libc::sighandler_t) == libc::SIG_ERR } {
        eprintln!(
            "distinst: signal handling error: {}",
            io::Error::last_os_error()
        );
        exit(1);
    }
}

fn parse_part_type(table: &str) -> Result<PartitionType, DistinstError> {
    match table {
        "primary" => Ok(PartitionType::Primary),
        "logical" => Ok(PartitionType::Logical),
        _ => Err(DistinstError::InvalidPartitionType),
    }
}

enum PartType {
    /// A normal partition with a standard file system
    Fs(FileSystemType),
    /// A partition that is formatted with LVM, optionally with encryption.
    Lvm(String, Option<LvmEncryption>),
}

fn parse_key(
    key: &str,
    pass: &mut Option<String>,
    keydata: &mut Option<String>,
) -> Result<(), DistinstError> {
    if key.starts_with("pass=") {
        let passval = &key[5..];
        if passval.is_empty() {
            return Err(DistinstError::EmptyPassword);
        }

        *pass = Some(passval.into());
    } else if key.starts_with("keyfile=") {
        let keyval = &key[8..];
        if keyval.is_empty() {
            return Err(DistinstError::EmptyKeyValue);
        }

        *keydata = Some(keyval.into());
    } else {
        return Err(DistinstError::InvalidField { field: key.into() });
    }

    Ok(())
}

fn parse_fs(fs: &str) -> Result<PartType, DistinstError> {
    if fs.starts_with("enc=") {
        let (mut pass, mut keydata) = (None, None);

        let mut fields = fs[4..].split(",");
        let physical_volume = fields
            .next()
            .map(|pv| pv.into())
            .ok_or(DistinstError::NoPhysicalVolume)?;

        let volume_group = fields
            .next()
            .map(|vg| vg.into())
            .ok_or(DistinstError::NoVolumeGroup)?;

        for field in fields {
            parse_key(field, &mut pass, &mut keydata);
        }

        Ok(PartType::Lvm(
            volume_group,
            if !pass.is_some() && !keydata.is_some() {
                None
            } else {
                Some(LvmEncryption::new(physical_volume, pass, keydata))
            },
        ))
    } else if fs.starts_with("lvm=") {
        let mut fields = fs[4..].split(",");
        Ok(PartType::Lvm(
            fields
                .next()
                .map(|vg| vg.into())
                .ok_or(DistinstError::NoVolumeGroup)?,
            None,
        ))
    } else {
        fs.parse::<FileSystemType>()
            .map(PartType::Fs)
            .ok()
            .ok_or_else(|| DistinstError::InvalidFileSystem { fs: fs.into() })
    }
}

fn parse_sector(sector: &str) -> Result<Sector, DistinstError> {
    let result = if sector.ends_with("MiB") {
        sector[..sector.len() - 3]
            .parse::<i64>()
            .ok()
            .and_then(|mebibytes| {
                format!("{}M", (mebibytes * 1_048_576) / 1_000_000)
                    .parse::<Sector>()
                    .ok()
            })
    } else {
        sector.parse::<Sector>().ok()
    };

    result.ok_or_else(|| DistinstError::InvalidSectorValue {
        value: sector.into(),
    })
}

fn parse_flags(flags: &str) -> Vec<PartitionFlag> {
    // TODO: implement FromStr for PartitionFlag
    flags
        .split(',')
        .filter_map(|flag| match flag {
            "esp" => Some(PartitionFlag::PED_PARTITION_ESP),
            "boot" => Some(PartitionFlag::PED_PARTITION_BOOT),
            "root" => Some(PartitionFlag::PED_PARTITION_ROOT),
            "swap" => Some(PartitionFlag::PED_PARTITION_SWAP),
            "hidden" => Some(PartitionFlag::PED_PARTITION_HIDDEN),
            "raid" => Some(PartitionFlag::PED_PARTITION_RAID),
            "lvm" => Some(PartitionFlag::PED_PARTITION_LVM),
            "lba" => Some(PartitionFlag::PED_PARTITION_LBA),
            "hpservice" => Some(PartitionFlag::PED_PARTITION_HPSERVICE),
            "palo" => Some(PartitionFlag::PED_PARTITION_PALO),
            "prep" => Some(PartitionFlag::PED_PARTITION_PREP),
            "msft_reserved" => Some(PartitionFlag::PED_PARTITION_MSFT_RESERVED),
            "apple_tv_recovery" => Some(PartitionFlag::PED_PARTITION_APPLE_TV_RECOVERY),
            "diag" => Some(PartitionFlag::PED_PARTITION_DIAG),
            "legacy_boot" => Some(PartitionFlag::PED_PARTITION_LEGACY_BOOT),
            "msft_data" => Some(PartitionFlag::PED_PARTITION_MSFT_DATA),
            "irst" => Some(PartitionFlag::PED_PARTITION_IRST),
            _ => None,
        })
        .collect::<Vec<_>>()
}

fn find_disk_mut<'a>(disks: &'a mut Disks, block: &str) -> Result<&'a mut Disk, DistinstError> {
    disks
        .find_disk_mut(block)
        .ok_or_else(|| DistinstError::DiskNotFound { disk: block.into() })
}

fn find_partition_mut<'a>(
    disk: &'a mut Disk,
    partition: i32,
) -> Result<&'a mut PartitionInfo, DistinstError> {
    disk.get_partition_mut(partition)
        .ok_or_else(|| DistinstError::PartitionNotFound { partition })
}

fn configure_tables(disks: &mut Disks, tables: Option<Values>) -> Result<(), DistinstError> {
    if let Some(tables) = tables {
        for table in tables {
            let values: Vec<&str> = table.split(":").collect();
            if values.len() != 2 {
                return Err(DistinstError::TableArgs);
            }

            let disk = find_disk_mut(disks, values[0])?;
            match values[1] {
                "gpt" => disk.mklabel(PartitionTable::Gpt)?,
                "msdos" => disk.mklabel(PartitionTable::Msdos)?,
                _ => {
                    return Err(DistinstError::InvalidTable {
                        table: values[1].into(),
                    });
                }
            }
        }
    }

    Ok(())
}

fn configure_removed(disks: &mut Disks, ops: Option<Values>) -> Result<(), DistinstError> {
    if let Some(ops) = ops {
        for op in ops {
            let mut args = op.split(":");
            let block_dev = match args.next() {
                Some(disk) => disk,
                None => {
                    return Err(DistinstError::NoBlockArg);
                }
            };

            for part in args {
                let part_id = match part.parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => {
                        return Err(DistinstError::ArgNaN { arg: part.into() });
                    }
                };

                find_disk_mut(disks, block_dev)?.remove_partition(part_id as i32)?;
            }
        }
    }

    Ok(())
}

fn configure_moved(disks: &mut Disks, parts: Option<Values>) -> Result<(), DistinstError> {
    if let Some(parts) = parts {
        for part in parts {
            let values: Vec<&str> = part.split(":").collect();
            if values.len() != 4 {
                return Err(DistinstError::MoveArgs);
            }

            let (block, partition, start, end) = (
                values[0],
                values[1]
                    .parse::<u32>()
                    .map(|x| x as i32)
                    .ok()
                    .ok_or_else(|| DistinstError::ArgNaN {
                        arg: values[1].into(),
                    })?,
                match values[2] {
                    "none" => None,
                    value => Some(parse_sector(value)?),
                },
                match values[3] {
                    "none" => None,
                    value => Some(parse_sector(value)?),
                },
            );

            let disk = find_disk_mut(disks, block)?;
            if let Some(start) = start {
                let start = disk.get_sector(start);
                disk.move_partition(partition, start)?;
            }

            if let Some(end) = end {
                let end = disk.get_sector(end);
                disk.resize_partition(partition, end)?;
            }
        }
    }

    Ok(())
}

fn configure_reused(disks: &mut Disks, parts: Option<Values>) -> Result<(), DistinstError> {
    if let Some(parts) = parts {
        for part in parts {
            let values: Vec<&str> = part.split(":").collect();
            if values.len() < 3 || values.len() > 5 {
                return Err(DistinstError::ReusedArgs);
            }

            let (block_dev, part_id, fs) = (
                values[0],
                values[1]
                    .parse::<u32>()
                    .map(|id| id as i32)
                    .map_err(|_| DistinstError::ArgNaN {
                        arg: values[1].into(),
                    })?,
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
                    return Err(DistinstError::InvalidField {
                        field: (*value).into(),
                    });
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
                    PartType::Lvm(volume_group, encryption) => {
                        partition.set_volume_group(volume_group, encryption);
                        FileSystemType::Lvm
                    }
                };

                partition.format_with(fs);
            }

            if let Some(flags) = flags {
                partition.flags = flags;
            }
        }
    }

    Ok(())
}

fn configure_new(disks: &mut Disks, parts: Option<Values>) -> Result<(), DistinstError> {
    if let Some(parts) = parts {
        for part in parts {
            let values: Vec<&str> = part.split(":").collect();
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
                    return Err(DistinstError::InvalidField {
                        field: (*value).into(),
                    });
                }
            }

            let disk = find_disk_mut(disks, block)?;

            let start = disk.get_sector(start);
            let end = disk.get_sector(end);
            let mut builder = match fs {
                PartType::Lvm(volume_group, encryption) => {
                    PartitionBuilder::new(start, end, FileSystemType::Lvm)
                        .partition_type(kind)
                        .logical_volume(volume_group, encryption)
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

// Defines a new partition to assign to a volume group
struct LogicalArgs {
    // The group to create a partition on
    group: String,
    // The name of the partition
    name: String,
    // The length of the partition
    size: Sector,
    // The filesystem to assign to this partition
    fs: FileSystemType,
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
        let values: Vec<&str> = value.split(":").collect();
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
                return Err(DistinstError::InvalidField {
                    field: (*arg).into(),
                });
            }
        }

        action(LogicalArgs {
            group: values[0].into(),
            name: values[1].into(),
            size: parse_sector(values[2])?,
            fs: match parse_fs(values[3])? {
                PartType::Fs(fs) => fs,
                PartType::Lvm(..) => {
                    unimplemented!("LUKS on LVM is unsupported");
                }
            },
            mount,
            flags,
        })?;
    }

    Ok(())
}

fn configure_lvm(
    disks: &mut Disks,
    logical: Option<Values>,
    modify: Option<Values>,
    remove: Option<Values>,
    remove_all: bool,
) -> Result<(), DistinstError> {
    if remove_all {
        for device in disks.get_logical_devices_mut() {
            device.clear_partitions();
        }
    } else if let Some(remove) = remove {
        for value in remove {
            let values: Vec<&str> = value.split(":").collect();
            if values.len() != 2 {
                return Err(DistinstError::LogicalRemoveArgs);
            }

            let (group, volume) = (values[0], values[1]);
            let device = disks.get_logical_device_mut(group).ok_or(
                DistinstError::LogicalDeviceNotFound {
                    group: group.into(),
                },
            )?;

            device.remove_partition(volume)?;
        }
    }

    if let Some(modify) = modify {
        for value in modify {
            let values: Vec<&str> = value.split(":").collect();
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

            let device = disks.get_logical_device_mut(group).ok_or(
                DistinstError::LogicalDeviceNotFound {
                    group: group.into(),
                },
            )?;

            let partition = device.get_partition_mut(volume).ok_or(
                DistinstError::LogicalPartitionNotFound {
                    group:  group.into(),
                    volume: volume.into(),
                },
            )?;

            if let Some(fs) = fs {
                let fs = match fs {
                    PartType::Fs(fs) => fs,
                    PartType::Lvm(volume_group, encryption) => {
                        partition.set_volume_group(volume_group, encryption);
                        FileSystemType::Lvm
                    }
                };

                partition.format_and_keep_name(fs);
            }

            if let Some(mount) = mount {
                partition.set_mount(PathBuf::from(mount.to_owned()));
            }
        }
    }

    if let Some(logical) = logical {
        parse_logical(logical, |args| {
            match disks.get_logical_device_mut(&args.group) {
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
                None => Err(DistinstError::NoVolumeGroupAssociated {
                    group: args.group.into(),
                }),
            }
        })?;
    }

    Ok(())
}

fn configure_decrypt(disks: &mut Disks, decrypt: Option<Values>) -> Result<(), DistinstError> {
    if let Some(decrypt) = decrypt {
        for device in decrypt {
            let values: Vec<&str> = device.split(':').collect();
            if values.len() != 3 {
                return Err(DistinstError::DecryptArgs);
            }

            let (device, pv) = (Path::new(values[0]), values[1].into());

            let (mut pass, mut keydata) = (None, None);
            parse_key(&values[2], &mut pass, &mut keydata)?;

            disks.decrypt_partition(device, LvmEncryption::new(pv, pass, keydata));
        }
    }

    Ok(())
}

fn configure_disks(matches: &ArgMatches) -> Result<Disks, DistinstError> {
    let mut disks = Disks::new();

    for block in matches.values_of("disk").unwrap() {
        eprintln!("distinst: adding {} to disks configuration", block);
        disks.add(Disk::from_name(block)?);
    }

    configure_tables(&mut disks, matches.values_of("table"))?;
    configure_removed(&mut disks, matches.values_of("delete"))?;
    eprintln!("distinst: configuring moved partitions");
    configure_moved(&mut disks, matches.values_of("move"))?;
    eprintln!("distinst: configuring reused partitions");
    configure_reused(&mut disks, matches.values_of("use"))?;
    eprintln!("distinst: configuring new partitions");
    configure_new(&mut disks, matches.values_of("new"))?;
    eprintln!("distinst: initializing LVM groups");
    disks
        .initialize_volume_groups()
        .map_err(|why| DistinstError::InitializeVolumes { why })?;
    eprintln!("distinst: handling pre-existing LUKS partitions");
    configure_decrypt(&mut disks, matches.values_of("decrypt"))?;
    eprintln!("distinst: configuring LVM devices");
    configure_lvm(
        &mut disks,
        matches.values_of("logical"),
        matches.values_of("logical-modify"),
        matches.values_of("logical-remove"),
        matches.is_present("logical-remove-all"),
    )?;
    eprintln!("distinst: disks configured");

    Ok(disks)
}
