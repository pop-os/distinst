extern crate distinst;
extern crate pbr;

use pbr::ProgressBar;

use distinst::*;
use distinst::auto::*;

use std::env;
use std::path::Path;
use std::cell::RefCell;
use std::rc::Rc;
use std::io;

fn main() {
    let mut args = env::args().skip(1);
    let disk = args.next().unwrap();
    let disk = Path::new(&disk);

    let pb_opt: Rc<RefCell<Option<ProgressBar<io::Stdout>>>> = Rc::new(RefCell::new(None));

    let mut disks = Disks::probe_devices().unwrap();

    let options = InstallOptions::new(&disks, 0);

    match options.erase_options.iter().find(|opt| &opt.device == disk) {
        Some(option) => {
            let option = InstallOption::EraseAndInstall {
                option,
                password: args.next()
            };

            match option.apply(&mut disks) {
                Ok(()) => {
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

                    let result = installer.install(
                        disks,
                        &Config {
                            flags: distinst::MODIFY_BOOT_ORDER | distinst::INSTALL_HARDWARE_SUPPORT,
                            hostname:         "pop-testing".into(),
                            keyboard_layout:  "us".into(),
                            keyboard_model:   None,
                            keyboard_variant: None,
                            old_root:         None,
                            lang:             "en_US".into(),
                            remove:           "/cdrom/casper/filesystem.manifest-remove".into(),
                            squashfs:         "/cdrom/casper/filesystem.squashfs".into(),
                        },
                    );

                    match result {
                        Ok(()) => (),
                        Err(why) => {
                            eprintln!("install failed: {}", why);
                        }
                    }
                }
                Err(why) => {
                    eprintln!("failed to apply: {}", why);
                }
            }
        }
        None => {
            eprintln!("erase option not found for {}", disk.display());
        }
    }
}
