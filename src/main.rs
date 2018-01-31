extern crate clap;
extern crate distinst;
extern crate libc;
extern crate pbr;

use clap::{App, Arg, ArgMatches};
use distinst::{
    Config, Disk, DiskError, Disks, FileSystemType, Installer, PartitionBuilder,
    PartitionFlag, PartitionTable, PartitionType, Sector, Step, KILL_SWITCH,
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
            .long("--squashfs")
            .help("define the squashfs image which will be installed")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("hostname")
            .short("h")
            .long("hostname")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("keyboard")
            .short("k")
            .long("keyboard")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("lang")
            .short("l")
            .long("lang")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("remove")
            .short("r")
            .long("remove")
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("disk")
            .short("b")
            .long("block")
            .takes_value(true)
            .multiple(true)
            .required(true)
        )
        .arg(Arg::with_name("table")
            .short("t")
            .long("new-table")
            .multiple(true)
            .takes_value(true)
        )
        .arg(Arg::with_name("new")
            .short("n")
            .long("new")
            .multiple(true)
            .takes_value(true)
        )
        // .arg(Arg::with_name("reuse")
        //     .short("u")
        //     .long("use")
        //     .takes_value(true)
        //     .multiple(true)
        // )
        // .arg(Arg::with_name("delete")
        //     .short("d")
        //     .long("delete")
        //     .takes_value(true)
        //     .multiple(true)
        // )
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

        // Set up signal handling before starting the installation process.
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

fn configure_disks(matches: &ArgMatches) -> Result<Disks, DiskError> {
    let mut disks = Disks(Vec::new());  

    for block in matches.values_of("disk").unwrap() {
        disks.0.push(Disk::from_name(block)?);
    }

    for table in matches.values_of("table").unwrap() {
        let values: Vec<&str> = table.split(":").collect();
        eprintln!("table values: {:?}", values);
        if values.len() != 2 {
            eprintln!("distinst: table argument requires two values");
            exit(1);
        }

        match disks.find_disk_mut(values[0]) {
            Some(mut disk) => match values[1] {
                "gpt" => disk.mklabel(PartitionTable::Msdos)?,
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

    for part in matches.values_of("new").unwrap() {
        let values: Vec<&str> = part.split(":").collect();
        let (kind, start, end, fs, mount, flags) = (
            match values[1] {
                "primary" => PartitionType::Primary,
                "logical" => PartitionType::Logical,
                _ => {
                    eprintln!("distinst: partition type must be either 'primary' or 'logical'.");
                    exit(1);
                }
            },
            match values[2].parse::<Sector>() {
                Ok(sector) => sector,
                Err(_) => {
                    eprintln!("distinst: provided sector unit, '{}', was invalid", values[2]);
                    exit(1);
                }
            },
            match values[3].parse::<Sector>() {
                Ok(sector) => sector,
                Err(_) => {
                    eprintln!("distinst: provided sector unit, '{}', was invalid", values[2]);
                    exit(1);
                }
            },
            match values[4].parse::<FileSystemType>() {
                Ok(fs) => fs,
                Err(_) => {
                    eprintln!("distinst: provided file system, '{}', was invalid", values[4]);
                    exit(1);
                }
            },
            values.get(5).map(Path::new),
            // TODO: implement FromStr for PartitionFlag
            values.get(6).map(|flags| flags.split(',').filter_map(|flag| match flag {
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
            }).collect::<Vec<_>>()),
        );

        match disks.find_disk_mut(values[0]) {
            Some(mut disk) => {
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
            None => {
                eprintln!("distinst: '{}' could not be found", values[0]);
                exit(1);
            }
        }
    }

    Ok(disks)
    // let mut disk = Disk::from_name(path)?;
    // match Bootloader::detect() {
    //     Bootloader::Bios => {
    //         disk.mklabel(PartitionTable::Msdos)?;

    //         let start = disk.get_sector(Sector::Start);
    //         let end = disk.get_sector(Sector::End);
    //         disk.add_partition(
    //             PartitionBuilder::new(start, end, FileSystemType::Ext4)
    //                 .partition_type(PartitionType::Primary)
    //                 .flag(PartitionFlag::PED_PARTITION_BOOT)
    //                 .mount(Path::new("/").to_path_buf()),
    //         )?;
    //     }
    //     Bootloader::Efi => {
    //         disk.mklabel(PartitionTable::Gpt)?;

    //         let mut start = disk.get_sector(Sector::Start);
    //         let mut end = disk.get_sector(Sector::Megabyte(512));
    //         disk.add_partition(
    //             PartitionBuilder::new(start, end, FileSystemType::Fat32)
    //                 .partition_type(PartitionType::Primary)
    //                 .flag(PartitionFlag::PED_PARTITION_ESP)
    //                 .mount(Path::new("/boot/efi").to_path_buf())
    //                 .name("EFI".into()),
    //         )?;

    //         start = end;
    //         end = disk.get_sector(Sector::MegabyteFromEnd(0x1000));

    //         disk.add_partition(
    //             PartitionBuilder::new(start, end, FileSystemType::Ext4)
    //                 .partition_type(PartitionType::Primary)
    //                 .mount(Path::new("/").to_path_buf())
    //                 .name("Pop!_OS".into()),
    //         )?;

    //         start = end;
    //         end = disk.get_sector(Sector::End);

    //         disk.add_partition(
    //             PartitionBuilder::new(start, end, FileSystemType::Swap)
    //                 .partition_type(PartitionType::Primary),
    //         )?;
    //     }
    // }
}
