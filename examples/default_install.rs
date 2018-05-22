extern crate distinst;
extern crate pbr;

use pbr::ProgressBar;

use distinst::auto::*;
use distinst::*;

use std::cell::RefCell;
use std::env;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::rc::Rc;

fn main() {
    let mut args = env::args().skip(1);
    let action = args.next().unwrap();

    let pb_opt: Rc<RefCell<Option<ProgressBar<io::Stdout>>>> = Rc::new(RefCell::new(None));

    let mut disks = Disks::probe_devices().unwrap();

    let options = InstallOptions::new(&disks, 0);

    let mut config = Config {
        flags:            distinst::MODIFY_BOOT_ORDER | distinst::INSTALL_HARDWARE_SUPPORT,
        hostname:         "pop-testing".into(),
        keyboard_layout:  "us".into(),
        keyboard_model:   None,
        keyboard_variant: None,
        old_root:         None,
        lang:             "en_US.UTF-8".into(),
        remove:           "/cdrom/casper/filesystem.manifest-remove".into(),
        squashfs:         "/cdrom/casper/filesystem.squashfs".into(),
    };

    match action.as_str() {
        "erase" => {
            let disk = args.next().unwrap();
            let disk = Path::new(&disk);

            match options.erase_options.iter().find(|opt| &opt.device == disk) {
                Some(option) => {
                    let option = InstallOption::EraseOption {
                        option,
                        password: args.next(),
                    };

                    match option.apply(&mut disks) {
                        Ok(()) => (),
                        Err(why) => {
                            eprintln!("failed to apply: {}", why);
                            return;
                        }
                    }
                }
                None => {
                    eprintln!("erase option not found for {}", disk.display());
                    return;
                }
            }
        }
        "refresh" | "retain" => {
            for (id, option) in options.refresh_options.iter().enumerate() {
                println!("{}: {}", id, option);
            }

            let mut buff = String::new();
            let option = loop {
                let _ = io::stdout()
                    .write_all(b"Select an option: ")
                    .and_then(|_| io::stdout().flush());
                let stdin = io::stdin();
                let _ = stdin.lock().read_line(&mut buff);
                if let Ok(number) = buff[..buff.len() - 1].parse::<usize>() {
                    break number;
                }

                buff.clear();
            };

            match options.refresh_options.get(option) {
                Some(option) => {
                    if action.as_str() == "retain" {
                        config.old_root = Some(option.root_part.clone());
                    }
                    match InstallOption::RefreshOption(option).apply(&mut disks) {
                        Ok(()) => (),
                        Err(why) => {
                            eprintln!("failed to apply: {}", why);
                            return;
                        }
                    }
                }
                None => {
                    eprintln!("index out of range");
                    return;
                }
            }
        }
        _ => {
            eprintln!("invalid action");
            return;
        }
    }

    println!("installing with {:#?}", disks);

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

    let _ = log(|level, msg| eprintln!("{:?}: {}", level, msg));

    match installer.install(disks, &config) {
        Ok(()) => (),
        Err(why) => {
            eprintln!("install failed: {}", why);
        }
    }
}
