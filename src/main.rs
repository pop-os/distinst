extern crate clap;
extern crate distinst;
extern crate pbr;

use clap::{App, Arg};
use distinst::{Config, Installer, Step};
use pbr::ProgressBar;

use std::{io, process};
use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    let matches = App::new("distinst")
        .arg(
            Arg::with_name("squashfs")
                .required(true)
        )
        .arg(
            Arg::with_name("drive")
                .required(true)
        )
        .get_matches();

    if let Err(err) = distinst::log("distinst") {
        println!("Failed to initialize logging: {}", err);
    }

    let squashfs = matches.value_of("squashfs").unwrap();
    let drive = matches.value_of("drive").unwrap();

    let pb_opt: Rc<RefCell<Option<ProgressBar<io::Stdout>>>> = Rc::new(RefCell::new(None));

    let res = {
        let mut installer = Installer::new();

        {
            let pb_opt = pb_opt.clone();
            installer.on_error(move |error| {
                if let Some(mut pb) = pb_opt.borrow_mut().take() {
                    pb.finish_println("");
                }

                println!("Error: {:?}", error);
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
                        Step::Partition => "Partitioning disk ",
                        Step::Format => "Formatting partitions ",
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

        installer.install(&Config {
            squashfs: squashfs.to_string(),
            drive: drive.to_string(),
        })
    };

    if let Some(mut pb) = pb_opt.borrow_mut().take() {
        pb.finish_println("");
    }

    match res {
        Ok(()) => {
            println!("Install was successful");
            process::exit(0);
        },
        Err(err) => {
            println!("Install failed: {}", err);
            process::exit(1);
        }
    }
}
