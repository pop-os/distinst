extern crate clap;
extern crate distinst;
extern crate pbr;

use clap::{App, Arg};
use distinst::{
    Bootloader, Config, Disk, DiskError, Disks, FileSystemType, Installer, PartitionBuilder,
    PartitionFlag, PartitionTable, PartitionType, Sector, Step,
};
use pbr::ProgressBar;

use std::{io, process};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

fn main() {
    let matches = App::new("distinst")
        .arg(Arg::with_name("squashfs").required(true))
        .arg(Arg::with_name("lang").required(true))
        .arg(Arg::with_name("remove").required(true))
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
            .takes_value(true)
            .multiple(true)
        )
        .arg(Arg::with_name("new")
            .short("n")
            .long("new")
            .takes_value(true)
            .multiple(true)
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
    let disk = matches.value_of("disk").unwrap();
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

        let disk = match configure_disk(disk) {
            Ok(disk) => disk,
            Err(why) => {
                eprintln!("distinst: invalid disk configuration: {}", why);
                process::exit(1);
            }
        };

        installer.install(
            Disks(vec![disk]),
            &Config {
                squashfs: squashfs.to_string(),
                lang:     lang.to_string(),
                remove:   remove.to_string(),
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

fn configure_disk(path: &str) -> Result<Disk, DiskError> {
    let mut disk = Disk::from_name(path)?;
    match Bootloader::detect() {
        Bootloader::Bios => {
            disk.mklabel(PartitionTable::Msdos)?;

            let start = disk.get_sector(Sector::Start);
            let end = disk.get_sector(Sector::End);
            disk.add_partition(
                PartitionBuilder::new(start, end, FileSystemType::Ext4)
                    .partition_type(PartitionType::Primary)
                    .flag(PartitionFlag::PED_PARTITION_BOOT)
                    .mount(Path::new("/").to_path_buf()),
            )?;
        }
        Bootloader::Efi => {
            disk.mklabel(PartitionTable::Gpt)?;

            let mut start = disk.get_sector(Sector::Start);
            let mut end = disk.get_sector(Sector::Megabyte(150));
            disk.add_partition(
                PartitionBuilder::new(start, end, FileSystemType::Fat32)
                    .partition_type(PartitionType::Primary)
                    .flag(PartitionFlag::PED_PARTITION_ESP)
                    .mount(Path::new("/boot/efi").to_path_buf()),
            )?;

            start = end;
            end = disk.get_sector(Sector::End);
            disk.add_partition(
                PartitionBuilder::new(start, end, FileSystemType::Ext4)
                    .partition_type(PartitionType::Primary)
                    .mount(Path::new("/").to_path_buf()),
            )?;
        }
    }

    Ok(disk)
}
