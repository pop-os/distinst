use disk_types::PartitionTable;
use external::wipefs;
use libparted::{Disk as PedDisk, DiskType as PedDiskType};
use crate::parted::*;
use std::{
    fs::File,
    io::{self, Seek, SeekFrom, Write},
    path::Path,
};

/// Writes a new partition table to the disk, clobbering it in the process.
pub fn mklabel<P: AsRef<Path>>(device_path: P, kind: PartitionTable) -> io::Result<()> {
    let _ = wipefs(&device_path);

    info!("writing {:?} table on {}", kind, device_path.as_ref().display());

    open_device(&device_path).and_then(|mut device| {
        let kind = match kind {
            PartitionTable::Gpt => PedDiskType::get("gpt").unwrap(),
            PartitionTable::Msdos => PedDiskType::get("msdos").unwrap(),
        };

        let device_path = device.path().to_path_buf();

        PedDisk::new_fresh(&mut device, kind)
            .map_err(|why| {
                io::Error::new(
                    why.kind(),
                    format!("failed to create partition table on {:?}: {}", device_path, why),
                )
            })
            .and_then(|mut disk| {
                commit(&mut disk).and_then(|_| sync(&mut unsafe { disk.get_device() }))
            })
    })?;

    Ok(())
}

