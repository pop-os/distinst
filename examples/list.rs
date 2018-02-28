extern crate distinst;

use distinst::{DiskExt, Disks};
use std::io::Result;
use std::process;

fn list() -> Result<()> {
    let disks = Disks::probe_devices()?;
    for disk in disks.get_physical_devices() {
        let sector_size = disk.get_sector_size();
        println!(
            "{}: {{ {}: {} MB ({} sectors) }}",
            disk.get_device_path().display(),
            disk.get_device_type(),
            (disk.get_sectors() * sector_size) / 1_000_000,
            disk.get_sectors()
        );

        for part in disk.get_partitions() {
            println!(
                "  {}:\n    start: {}\n    end:   {}\n    size:  {} MB ({} MiB)\n    fs:    {:?} \
                \n    usage: {}",
                part.device_path.display(),
                part.start_sector,
                part.end_sector,
                (part.sectors() * sector_size) / 1_000_000,
                (part.sectors() * sector_size) / 1_048_576,
                part.filesystem,
                if let Some(result) = part.sectors_used(sector_size) {
                    match result {
                        Ok(used_sectors) => {
                            let used = used_sectors * sector_size;
                            format!(
                                "{}%: {} MB ({} MiB)",
                                ((used_sectors as f64 / part.sectors() as f64) * 100f64) as u8,
                                used / 1_000_000,
                                used / 1_048_576
                            )
                        }
                        Err(why) => {
                            eprintln!(
                                "list: error getting usage for {} ({:?}): {}",
                                part.device_path.display(),
                                part.filesystem,
                                why
                            );
                            ::std::process::exit(1);
                        }
                    }
                } else {
                    "N/A".into()
                }
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
