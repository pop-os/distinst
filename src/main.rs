extern crate clap;
extern crate distinst;
extern crate libc;
extern crate pbr;

use clap::{App, Arg, ArgMatches};
use distinst::{
    Config, Disk, DiskError, Disks, FileSystemType, Installer, PartitionBuilder,
    PartitionFlag, PartitionInfo, PartitionTable, PartitionType, Sector, Step, KILL_SWITCH,
    PARTITIONING_TEST
};
use pbr::ProgressBar;

use std::{io, process};
use std::cell::RefCell;
use std::path::Path;
use std::process::exit;
use std::rc::Rc;
use std::sync::atomic::Ordering;

fn main() {
    let matches = App::new("distinst")
        .arg(Arg::with_name("squashfs")
            .short("s")
            .long("squashfs")
            .help("define the squashfs image which will be installed")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("hostname")
            .short("h")
            .long("hostname")
            .help("define the hostname that the new system will have")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("keyboard")
            .short("k")
            .long("keyboard")
            .help("define the keyboard configuration to use")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("lang")
            .short("l")
            .long("lang")
            .help("define the locale that the new system will use")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("remove")
            .short("r")
            .long("remove")
            .help("defines the manifest file that contains the packages to remove post-install")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("disk")
            .short("b")
            .long("block")
            .help("defines a disk that will be manipulated in the installation process")
            .takes_value(true)
            .multiple(true)
            .required(true)
        )
        .arg(Arg::with_name("table")
            .short("t")
            .long("new-table")
            .help("defines a new partition table to apply to the disk, \
                clobbering it in the process")
            .multiple(true)
            .takes_value(true)
        )
        .arg(Arg::with_name("new")
            .short("n")
            .long("new")
            .help("defines a new partition that will be created on the disk")
            .multiple(true)
            .takes_value(true)
        )
        .arg(Arg::with_name("use")
            .short("u")
            .long("use")
            .help("defines to reuse an existing partition on the disk")
            .takes_value(true)
            .multiple(true)
        )
        .arg(Arg::with_name("test").long("test"))
        .arg(Arg::with_name("delete")
            .short("d")
            .long("delete")
            .takes_value(true)
            .multiple(true)
        )
        // .arg(Arg::with_name("move")
        //     .short("m")
        //     .long("move")
        //     .takes_value(true)
        //     .multiple(true)
        // )
        .get_matches();

    if let Err(err) = distinst::log(|_level, message| {
        println!("{}", message);
    }) {
        eprintln!("Failed to initialize logging: {}", err);
    }

    let squashfs = matches.value_of("squashfs").unwrap();
    let hostname = matches.value_of("hostname").unwrap();
    let keyboard = matches.value_of("keyboard").unwrap();
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
                eprintln!("distinst: invalid disk configuration: {}", why);
                process::exit(1);
            }
        };

        configure_signal_handling();
        if matches.occurrences_of("test") != 0 {
            PARTITIONING_TEST.store(true, Ordering::SeqCst);
        }

        installer.install(
            disks,
            &Config {
                hostname: hostname.into(),
                keyboard: keyboard.into(),
                lang:     lang.into(),
                remove:   remove.into(),
                squashfs: squashfs.into(),
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

    process::exit(status);
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
        process::exit(1);
    }
}

fn parse_part_type(table: &str) -> PartitionType {
    match table {
        "primary" => PartitionType::Primary,
        "logical" => PartitionType::Logical,
        _ => {
            eprintln!("distinst: partition type must be either 'primary' or 'logical'.");
            exit(1);
        }
    }
}

fn parse_fs(fs: &str) -> FileSystemType {
    match fs.parse::<FileSystemType>() {
        Ok(fs) => fs,
        Err(_) => {
            eprintln!("distinst: provided file system, '{}', was invalid", fs);
            exit(1);
        }
    }
}

fn parse_sector(sector: &str) -> Sector {
    match sector.parse::<Sector>() {
        Ok(sector) => sector,
        Err(_) => {
            eprintln!("distinst: provided sector unit, '{}', was invalid", sector);
            exit(1);
        }
    }
}

fn parse_flags(flags: &str) -> Vec<PartitionFlag> {
    // TODO: implement FromStr for PartitionFlag
    flags.split(',').filter_map(|flag| match flag {
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
        _ => None
    }).collect::<Vec<_>>()
}

fn find_disk_mut<'a>(disks: &'a mut Disks, block: &str) -> &'a mut Disk {
    match disks.find_disk_mut(block) {
        Some(disk) => disk,
        None => {
            eprintln!("distinst: '{}' could not be found", block);
            exit(1);
        }
    }
}

fn find_partition_mut<'a>(disk: &'a mut Disk, part_id: i32) -> &'a mut PartitionInfo {
    match disk.get_partition_mut(part_id) {
        Some(partition) => partition,
        None => {
            eprintln!("distinst: partition '{}' was not found", part_id);
            exit(1);
        }
    }
}

fn configure_disks(matches: &ArgMatches) -> Result<Disks, DiskError> {
    let mut disks = Disks(Vec::new());

    for block in matches.values_of("disk").unwrap() {
        disks.0.push(Disk::from_name(block)?);
    }

    if let Some(tables) = matches.values_of("table") {
        for table in tables {
            let values: Vec<&str> = table.split(":").collect();
            eprintln!("table values: {:?}", values);
            if values.len() != 2 {
                eprintln!("distinst: table argument requires two values");
                exit(1);
            }

            match disks.find_disk_mut(values[0]) {
                Some(mut disk) => match values[1] {
                    "gpt" => disk.mklabel(PartitionTable::Gpt)?,
                    "msdos" => disk.mklabel(PartitionTable::Msdos)?,
                    _ => {
                        eprintln!("distinst: '{}' is not valid. \
                            Value must be either 'gpt' or 'msdos'.", values[1]);
                        exit(1);
                    }
                }
                None => {
                    eprintln!("distinst: '{}' could not be found", values[0]);
                    exit(1);
                }
            }
        }
    }

    if let Some(ops) = matches.values_of("delete") {
        for op in ops {
            let mut args = op.split(":");
            let block_dev = match args.next() {
                Some(disk) => disk,
                None => {
                    eprintln!("distinst: no block argument provided");
                    exit(1);
                }
            };

            for part in args {
                let part_id = match part.parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => {
                        eprintln!("distinst: argument is not a valid number");
                        exit(1);
                    }
                };

                let disk = find_disk_mut(&mut disks, block_dev);
                let mut partition = find_partition_mut(disk, part_id as i32);
                partition.remove();
            }
        }
    }

    if let Some(parts) = matches.values_of("use") {
        for part in parts {
            let values: Vec<&str> = part.split(":").collect();
            if values.len() < 3 {
                eprintln!("distinst: three to five colon-delimited values need to be supplied to \
                --use\n\t-u USAGE: 'part_block:fs-or-reuse:mount[:flags,...]'");
                exit(1);
            } else if values.len() > 5 {
                eprintln!("distinst: too many values were supplied to the use partition flag.");
                exit(1);
            }

            let (block_dev, part_id, fs, mount, flags) = (
                values[0],
                match values[1].parse::<u32>() {
                    Ok(id) => id as i32,
                    Err(_) => {
                        eprintln!("distinst: partition value must be a number");
                        exit(1);
                    }
                },
                match values[2] {
                    "reuse" => None,
                    fs => Some(parse_fs(fs)),
                },
                values.get(3),
                values.get(4).map(|&flags| parse_flags(flags))
            );

            let disk = find_disk_mut(&mut disks, block_dev);
            let mut partition = find_partition_mut(disk, part_id);

            if let Some(mount) = mount {
                partition.set_mount(Path::new(mount).to_path_buf());
            }

            if let Some(fs) = fs {
                partition.format_with(fs);
            }

            if let Some(flags) = flags {
                partition.flags = flags;
            }
        }
    }

    if let Some(parts) = matches.values_of("new") {
        for part in parts {
            let values: Vec<&str> = part.split(":").collect();
            if values.len() < 5 {
                eprintln!("distinst: five to seven colon-delimited values need to be supplied \
                    to a new partition.\n\t-n USAGE: \
                    'block:part_type:start_sector:end_sector:fs[:mount:flags,...]'");
                exit(1);
            } else if values.len() > 7 {
                eprintln!("distinst: too many values were supplied to the new partition flag");
                exit(1);
            }

            let (block, kind, start, end, fs, mount, flags) = (
                values[0],
                parse_part_type(values[1]),
                parse_sector(values[2]),
                parse_sector(values[3]),
                parse_fs(values[4]),
                values.get(5).map(Path::new),
                values.get(6).map(|&flags| parse_flags(flags)),
            );

            let mut disk = find_disk_mut(&mut disks, block);

            let start = disk.get_sector(start);
            let end = disk.get_sector(end);
            let mut builder = PartitionBuilder::new(start, end, fs).partition_type(kind);

            if let Some(mount) = mount {
                builder = builder.mount(mount.into());
            }

            if let Some(flags) = flags {
                for flag in flags {
                    builder = builder.flag(flag);
                }
            }

            disk.add_partition(builder)?;
        }
    }

    Ok(disks)
}
