extern crate distinst;

use distinst::{Disks, InstallOptions};
use std::io::Result;
use std::process;

fn list() -> Result<()> {
    let mut disks = Disks::probe_devices()?;
    let _ = disks.initialize_volume_groups();
    let recommended = InstallOptions::detect(&disks, 5_000_000_000 / 512).unwrap();

    println!("Recommended Install Options:");
    println!("    largest size: {}", recommended.largest_available);
    println!("    largest option: {}", recommended.largest_option);

    if let Some(efi_parts) = recommended.efi_partitions {
        println!("    Pre-existing EFI Partitions:");
        for efi_part in &efi_parts {
            println!("        {:?}", efi_part);
        }
    }

    println!("    Install Locations:");
    for option in &recommended.options {
        println!(
            "        {} ({} sectors): {:?}",
            option.path.display(),
            option.install_size,
            option.kind
        );
    }

    Ok(())
}

fn main() {
    if let Err(err) = list() {
        eprintln!("list: failed: {}", err);
        process::exit(1);
    }
}
