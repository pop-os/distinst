extern crate clap;
extern crate distinst;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate libc;
extern crate pbr;

mod configure;
mod errors;

use clap::{App, Arg, ArgMatches, Values};
use configure::*;
use distinst::{timezones::Timezones, *};
use errors::DistinstError;

use pbr::ProgressBar;

use std::{
    cell::RefCell,
    io,
    path::{Path, PathBuf},
    process::exit,
    rc::Rc,
    sync::atomic::Ordering,
};

fn main() {
    let matches = App::new("distinst")
        .arg(
            Arg::with_name("username")
                .long("username")
                .requires("profile_icon")
                .help("specifies a default user account to create")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("password")
                .long("password")
                .help("set the password for the username")
                .requires("username")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("realname")
                .long("realname")
                .help("the full name of user to create")
                .requires("username")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("profile_icon")
                .long("profile_icon")
                .help("path to icon for user profile")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("timezone")
                .long("tz")
                .help("the timezone to set for the new install")
                .value_delimiter("/")
                .min_values(2)
                .max_values(2),
        )
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
            Arg::with_name("hardware-support")
                .long("hardware-support")
                .help("install hardware support packages based on detected hardware"),
        )
        .arg(
            Arg::with_name("modify-boot")
                .long("modify-boot")
                .help("modify the boot order after installing"),
        )
        .arg(
            Arg::with_name("force-bios")
                .long("force-bios")
                .help("performs a BIOS installation even if the running system is EFI"),
        )
        .arg(
            Arg::with_name("force-efi")
                .long("force-efi")
                .help("performs an EFI installation even if the running system is BIOS"),
        )
        .arg(
            Arg::with_name("no-efi-vars")
                .long("no-efi-vars")
                .help("disables mounting of the efivars directory"),
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
        .arg(
            Arg::with_name("run-ubuntu-drivers")
                .long("run-ubuntu-drivers")
                .help("use ubuntu-drivers to find drivers then install in the chroot, some may have proprietary licenses")
        )
        .get_matches();

    if let Err(err) = distinst::log(|_level, _message| {}) {
        eprintln!("Failed to initialize logging: {}", err);
    }

    let squashfs = matches.value_of("squashfs").unwrap();
    let hostname = matches.value_of("hostname").unwrap();
    let mut keyboard = matches.values_of("keyboard").unwrap();
    let lang = matches.value_of("lang").unwrap();
    let remove = matches.value_of("remove").unwrap();

    let tzs_;
    let timezone = match matches.values_of("timezone") {
        Some(mut tz) => {
            let (zone, region) = (tz.next().unwrap(), tz.next().unwrap());
            tzs_ = Timezones::new().expect("failed to get timzones");
            let zone = tzs_
                .zones()
                .into_iter()
                .find(|z| z.name() == zone)
                .expect(&format!("failed to find zone: {}", zone));
            let region = zone
                .regions()
                .into_iter()
                .find(|r| r.name() == region)
                .expect(&format!("failed to find region: {}", region));
            Some(region.clone())
        }
        None => None,
    };

    let user_account = matches.value_of("username").map(|username| {
        let username = username.to_owned();
        let profile_icon = matches.value_of("profile_icon").map(String::from);

        let realname = matches.value_of("realname").map(String::from);
        let password = matches.value_of("password").map(String::from).or_else(|| {
            if unsafe { libc::isatty(0) } == 0 {
                let mut pass = String::new();
                io::stdin().read_line(&mut pass).unwrap();
                pass.pop();
                Some(pass)
            } else {
                None
            }
        });

        UserAccountCreate { realname, username, password, profile_icon }
    });

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
                        Step::Backup => "Backing up files",
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

        if let Some(timezone) = timezone {
            installer.set_timezone_callback(move || timezone.clone());
        }

        if let Some(user_account) = user_account {
            installer.set_user_callback(move || user_account.clone());
        }

        let disks = match configure_disks(&matches) {
            Ok(disks) => disks,
            Err(why) => {
                eprintln!("distinst: {}", why);
                exit(1);
            }
        };

        configure_signal_handling();

        if matches.is_present("test") {
            PARTITIONING_TEST.store(true, Ordering::Relaxed);
        }

        if matches.is_present("force-bios") {
            FORCE_BOOTLOADER.store(1, Ordering::Relaxed);
        } else if matches.is_present("force-efi") {
            FORCE_BOOTLOADER.store(2, Ordering::Relaxed);
        }

        if matches.is_present("no-efi-vars") {
            NO_EFI_VARIABLES.store(true, Ordering::Relaxed);
        }

        fn take_optional_string(argument: Option<&str>) -> Option<String> {
            argument.map(String::from).and_then(|x| if x.is_empty() { None } else { Some(x) })
        }

        // The lock is an `OwnedFd`, which on drop will close / unlock the inhibitor.
        let _inhibit_suspend = match distinst::dbus_interfaces::LoginManager::new() {
            Ok(manager) => match manager.connect().inhibit_suspend(
                "Distinst Installer",
                "prevent suspension while installing a distribution",
            ) {
                Ok(lock) => Some(lock),
                Err(why) => {
                    eprintln!("distinst: failed to inhibit suspend: {}", why);
                    None
                }
            },
            Err(why) => {
                eprintln!("distinst: failed to get logind dbus connection: {}", why);
                None
            }
        };

        installer.install(
            disks,
            &Config {
                flags:            install_flags(&matches),
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

fn install_flags(matches: &ArgMatches) -> u8 {
    let mut flags = 0;

    flags +=
        if matches.occurrences_of("modify-boot") != 0 { distinst::MODIFY_BOOT_ORDER } else { 0 };

    flags += if matches.occurrences_of("hardware-support") != 0 {
        distinst::INSTALL_HARDWARE_SUPPORT
    } else {
        0
    };

    flags += if matches.occurrences_of("run-ubuntu-drivers") != 0 {
        distinst::RUN_UBUNTU_DRIVERS
    } else {
        0
    };

    flags
}

fn configure_signal_handling() {
    extern "C" fn handler(signal: i32) {
        match signal {
            libc::SIGINT => KILL_SWITCH.store(true, Ordering::SeqCst),
            _ => unreachable!(),
        }
    }

    if unsafe { libc::signal(libc::SIGINT, handler as libc::sighandler_t) == libc::SIG_ERR } {
        eprintln!("distinst: signal handling error: {}", io::Error::last_os_error());
        exit(1);
    }
}

enum PartType {
    /// A normal partition with a standard file system
    Fs(Option<FileSystem>),
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

        let mut fields = fs[4..].split(',');
        let physical_volume =
            fields.next().map(|pv| pv.into()).ok_or(DistinstError::NoPhysicalVolume)?;

        let volume_group = fields.next().map(|vg| vg.into()).ok_or(DistinstError::NoVolumeGroup)?;

        for field in fields {
            parse_key(field, &mut pass, &mut keydata)?;
        }

        Ok(PartType::Lvm(
            volume_group,
            if pass.is_none() && keydata.is_none() {
                None
            } else {
                Some(LvmEncryption::new(physical_volume, pass, keydata))
            },
        ))
    } else if fs.starts_with("lvm=") {
        let mut fields = fs[4..].split(',');
        Ok(PartType::Lvm(
            fields.next().map(|vg| vg.into()).ok_or(DistinstError::NoVolumeGroup)?,
            None,
        ))
    } else {
        Ok(PartType::Fs(fs.parse::<FileSystem>().ok()))
    }
}

fn parse_sector(sector: &str) -> Result<Sector, DistinstError> {
    let result = if sector.ends_with("MiB") {
        sector[..sector.len() - 3].parse::<i64>().ok().and_then(|mebibytes| {
            format!("{}M", (mebibytes * 1_048_576) / 1_000_000).parse::<Sector>().ok()
        })
    } else {
        sector.parse::<Sector>().ok()
    };

    result.ok_or_else(|| DistinstError::InvalidSectorValue { value: sector.into() })
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
    disks.find_disk_mut(block).ok_or_else(|| DistinstError::DiskNotFound { disk: block.into() })
}

fn find_partition_mut(
    disk: &mut Disk,
    partition: i32,
) -> Result<&mut PartitionInfo, DistinstError> {
    disk.get_partition_mut(partition).ok_or_else(|| DistinstError::PartitionNotFound { partition })
}
