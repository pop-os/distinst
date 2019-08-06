extern crate distinst;

use distinst::{auto::InstallOptions, disks::Disks};

fn main() {
    let mut disks = Disks::probe_devices().unwrap();
    let _ = disks.initialize_volume_groups();

    let options = InstallOptions::new(&disks, 0, 0);

    println!("Refresh Options:");
    for (id, option) in options.refresh_options.iter().enumerate() {
        println!("  {} : {}", id, option);
    }

    println!("Erase and Install Options:");
    for (id, option) in options.erase_options.iter().enumerate() {
        println!("  {} : {}", id, option);
    }
}
