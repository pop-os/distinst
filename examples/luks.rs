extern crate distinst;
extern crate libparted;
use libparted::{Device, Disk};

fn main() {
    if let Ok(mut device) = Device::get("/dev/mapper/data") {
        eprintln!("found LUKS device: {:?}", device.type_());
        if let Ok(disk) = Disk::new(&mut device) {
            eprintln!("Opened LUKS device: {:?}", disk.get_disk_type_name());
            for part in disk.parts() {
                eprintln!("info: {:?}", distinst::PartitionInfo::new_from_ped(&part));
            }
        }
    }
}
