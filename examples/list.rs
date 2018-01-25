extern crate distinst;

use std::io::Result;
use std::process;

fn list() -> Result<()> {
    let installer = distinst::Installer::default();
    for disk in installer.disks()? {
        println!(
            "{}: {} MB",
            disk.device_path.display(),
            (disk.size * disk.sector_size) / 1_000_000
        );

        for part in disk.partitions {
            println!(
                "    {}: {} MB",
                part.device_path.display(),
                ((part.end_sector - part.start_sector) * disk.sector_size) / 1_000_000
            );
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
