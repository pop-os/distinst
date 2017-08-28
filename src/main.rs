extern crate distinst;

use std::io::Result;
use std::process;

fn installer() -> Result<()> {
    let installer = distinst::Installer::new();
    for disk in installer.disks()? {
        println!("{}: {} MB", disk.name(), disk.size()? / 1000000);

        for part in disk.parts()? {
            println!("    {}: {} MB", part.name(), part.size()? / 1000000);
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
