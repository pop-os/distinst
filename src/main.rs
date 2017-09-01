extern crate distinst;
extern crate pbr;

use distinst::{Config, Installer, Step};
use pbr::ProgressBar;

use std::{io, process};
use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    let pb_opt: Rc<RefCell<Option<ProgressBar<io::Stdout>>>> = Rc::new(RefCell::new(None));

    let res = {
        let mut installer = Installer::new();
        installer.on_error(|error| {
            println!("Error: {:?}", error);
        });

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
                    Step::Bootloader => "Installing bootloader ",
                });
                *pb_opt.borrow_mut() = Some(pb);
            }

            if let Some(ref mut pb) = *pb_opt.borrow_mut() {
                pb.set(status.percent as u64);
            }
        });

        installer.install(&Config {
            squashfs: "bash/filesystem.squashfs".to_string(),
            drive: "loop3".to_string(),
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
