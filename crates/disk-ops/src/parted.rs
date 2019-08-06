use bootloader::Bootloader;
use libparted::{Device, Disk as PedDisk, DiskType as PedDiskType};
use std::{io, path::Path};

/// Gets a `libparted::Device` from the given name.
pub fn get_device<'a, P: AsRef<Path>>(name: P) -> io::Result<Device<'a>> {
    let device = name.as_ref();
    info!("getting device at {}", device.display());
    Device::get(device).map_err(|why| {
        io::Error::new(
            why.kind(),
            format!("failed to get libparted device at {:?}: {}", device, why),
        )
    })
}

/// Gets and opens a `libparted::Device` from the given name.
pub fn open_device<'a, P: AsRef<Path>>(name: P) -> io::Result<Device<'a>> {
    let device = name.as_ref();
    info!("opening device at {}", device.display());
    Device::new(device).map_err(|why| {
        io::Error::new(
            why.kind(),
            format!("failed to open libparted device at {:?}: {}", device, why),
        )
    })
}

/// Opens a `libparted::Disk` from a `libparted::Device`.
pub fn open_disk<'a>(device: &'a mut Device) -> io::Result<PedDisk<'a>> {
    info!("opening disk at {}", device.path().display());
    let device = device as *mut Device;
    unsafe {
        match PedDisk::new(&mut *device) {
            Ok(disk) => Ok(disk),
            Err(_) => {
                info!("unable to open disk; creating new table on it");
                PedDisk::new_fresh(
                    &mut *device,
                    match Bootloader::detect() {
                        Bootloader::Bios => PedDiskType::get("msdos").unwrap(),
                        Bootloader::Efi => PedDiskType::get("gpt").unwrap(),
                    },
                )
                .map_err(|why| {
                    io::Error::new(
                        why.kind(),
                        format!(
                            "failed to create new partition table on {:?}: {}",
                            (&*device).path(),
                            why
                        ),
                    )
                })
            }
        }
    }
}

/// Attempts to commit changes to the disk, return a `DiskError` on failure.
pub fn commit(disk: &mut PedDisk) -> io::Result<()> {
    info!("committing changes to {}", unsafe { disk.get_device().path().display() });

    disk.commit().map_err(|why| {
        io::Error::new(
            why.kind(),
            format!(
                "failed to commit libparted changes to {:?}: {}",
                unsafe { disk.get_device() }.path(),
                why
            ),
        )
    })
}

/// Flushes the OS cache, return a `DiskError` on failure.
pub fn sync(device: &mut Device) -> io::Result<()> {
    info!("syncing device at {}", device.path().display());
    device.sync().map_err(|why| {
        io::Error::new(
            why.kind(),
            format!("failed to sync libparted device at {:?}: {}", device.path(), why),
        )
    })
}
