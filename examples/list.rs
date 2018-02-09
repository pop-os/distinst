extern crate distinst;

use distinst::Disks;
use std::io::Result;
use std::process;

fn list() -> Result<()> {
    for disk in Disks::probe_devices()? {
        println!(
            "{}: {{ {}: {} MB ({} sectors) }}",
            disk.device_path.display(),
            disk.device_type,
            (disk.size * disk.sector_size) / 1_000_000,
            disk.size
        );

        for part in disk.partitions {
            println!(
                "  {}:\n    start: {}\n    end:   {}\n    size:  {} MB ({} MiB)\n    fs:    {:?}",
                part.device_path.display(),
                part.start_sector,
                part.end_sector,
                ((part.end_sector + 1 - part.start_sector) * disk.sector_size) / 1_000_000,
                ((part.end_sector + 1 - part.start_sector) * disk.sector_size) / 1_048_576,
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
