extern crate distinst;

use distinst::{BlockDeviceExt, DiskExt, Disks, PartitionExt, SectorExt};
use std::{
    io::{self, Result},
    process,
};

fn list() -> Result<()> {
    let mut disks = Disks::probe_devices()?;
    let _ = disks.initialize_volume_groups();

    for disk in disks.get_physical_devices() {
        let sector_size = disk.get_sector_size();
        println!(
            "{}: {{ {}: {} MB ({} sectors) }}",
            disk.get_device_path().display(),
            disk.get_device_type(),
            (disk.get_sectors() * sector_size) / 1_000_000,
            disk.get_sectors()
        );

        println!("  removable: {}", disk.is_removable());

        for part in disk.get_partitions() {
            println!("  {}:", part.device_path.display());
            println!("    mount:   {:?}", part.mount_point);
            println!("    label:   {:?}", part.name);
            println!("    fs:      {:?}", part.filesystem);
            println!("    sectors: (start: {}, end: {})", part.start_sector, part.end_sector);
            println!(
                "    size:    {} MB ({} MiB)",
                (part.get_sectors() * sector_size) / 1_000_000,
                (part.get_sectors() * sector_size) / 1_048_576
            );

            println!(
                "    usage:   {}",
                match part.sectors_used() {
                    Ok(used_sectors) => {
                        let used = used_sectors * sector_size;
                        format!(
                            "{}%: {} MB ({} MiB)",
                            ((used_sectors as f64 / part.get_sectors() as f64) * 100f64) as u8,
                            used / 1_000_000,
                            used / 1_048_576
                        )
                    }
                    Err(ref why) if why.kind() == io::ErrorKind::NotFound => "N/A".into(),
                    Err(ref why) => {
                        eprintln!(
                            "list: error getting usage for {} ({:?}): {}",
                            part.device_path.display(),
                            part.filesystem,
                            why
                        );
                        ::std::process::exit(1);
                    }
                }
            );

            println!("    OS:      {:?}", part.probe_os());
        }
    }

    for disk in disks.get_logical_devices() {
        let sector_size = disk.get_sector_size();
        println!(
            "{}: {{ {}: {} MB ({} sectors) }}",
            disk.get_device_path().display(),
            "LVM Device Map",
            (disk.get_sectors() * sector_size) / 1_000_000,
            disk.get_sectors()
        );

        println!("  removable: {}", disk.is_removable());

        for part in disk.get_partitions() {
            println!("  {}:", part.device_path.display());
            println!("    mount:   {:?}", part.mount_point);
            println!("    label:   {:?}", part.name);
            println!("    fs:      {:?}", part.filesystem);
            println!("    sectors: (start: {}, end: {})", part.start_sector, part.end_sector);
            println!(
                "    size:    {} MB ({} MiB)",
                (part.get_sectors() * sector_size) / 1_000_000,
                (part.get_sectors() * sector_size) / 1_048_576
            );

            println!(
                "    usage:   {}",
                match part.sectors_used() {
                    Ok(used_sectors) => {
                        let used = used_sectors * sector_size;
                        format!(
                            "{}%: {} MB ({} MiB)",
                            ((used_sectors as f64 / part.get_sectors() as f64) * 100f64) as u8,
                            used / 1_000_000,
                            used / 1_048_576
                        )
                    }
                    Err(ref why) if why.kind() == io::ErrorKind::NotFound => "N/A".into(),
                    Err(ref why) => {
                        eprintln!(
                            "list: error getting usage for {} ({:?}): {}",
                            part.device_path.display(),
                            part.filesystem,
                            why
                        );
                        ::std::process::exit(1);
                    }
                }
            );

            println!("    OS:      {:?}", part.probe_os());
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
