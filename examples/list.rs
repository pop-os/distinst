extern crate distinst;

use distinst::{DiskExt, Disks};
use std::io::Result;
use std::process;

fn list() -> Result<()> {
    let disks = Disks::probe_devices()?;
    for disk in disks.get_physical_devices() {
        println!(
            "{}: {{ {}: {} MB ({} sectors) }}",
            disk.get_device_path().display(),
            disk.get_device_type(),
            (disk.get_sectors() * disk.get_sector_size()) / 1_000_000,
            disk.get_sectors()
        );

        for part in disk.get_partitions() {
            println!(
                "  {}:\n    start: {}\n    end:   {}\n    size:  {} MB ({} MiB)\n    fs:    {:?}",
                part.device_path.display(),
                part.start_sector,
                part.end_sector,
                (part.sectors() * disk.get_sector_size()) / 1_000_000,
                (part.sectors() * disk.get_sector_size()) / 1_048_576,
                part.filesystem
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
