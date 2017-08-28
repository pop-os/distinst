extern crate distinst;

use std::io::Result;
use std::process;

fn installer() -> Result<()> {
    let mut installer = distinst::Installer::new();
    for disk in installer.disks()? {
        println!("{}: {} MB", disk.name(), disk.size()? / 1024 / 1024);

        for part in disk.parts()? {
            println!("    {}: {} MB", part.name(), part.size()? / 1024 / 1024);
        }
    }

    Ok(())
}

fn main() {
    if let Err(err) = installer() {
        eprintln!("installer: failed to install: {}", err);
        process::exit(1);
    }
}
