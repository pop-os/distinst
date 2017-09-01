extern crate distinst;
extern crate pbr;

use distinst::{Config, Installer, Step};
use pbr::ProgressBar;

use std::{io, process};

fn main() {
    let mut installer = Installer::new();
    installer.on_error(|error| {
        println!("Error: {:?}", error);
        process::exit(1);
    });

    let mut step_opt = None;
    let mut pb_opt: Option<ProgressBar<io::Stdout>> = None;
    installer.on_status(move |status| {
        if step_opt != Some(status.step) {
            if let Some(mut pb) = pb_opt.take() {
                pb.finish_println("Finished");
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
            pb_opt = Some(pb);
        }

        if let Some(ref mut pb) = pb_opt {
            pb.set(status.percent as u64);
        }
    });

    installer.install(&Config {
        squashfs: "bash/filesystem.squashfs".to_string(),
        drive: "drive".to_string(),
    });
}
