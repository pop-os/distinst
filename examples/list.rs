extern crate distinst;

use std::io::Result;
use std::process;

fn list() -> Result<()> {
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
    if let Err(err) = list() {
        eprintln!("list: failed: {}", err);
        process::exit(1);
    }
}
