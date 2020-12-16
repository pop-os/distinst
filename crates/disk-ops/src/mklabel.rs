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
    let _ = zero(&device_path, 2047, 1);
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

/// Write sectors of zeroes to a block device
pub fn zero<P: AsRef<Path>>(device: P, sectors: u64, offset: u64) -> io::Result<()> {
    let zeroed_sector = [0; 512];
    File::open(device.as_ref()).and_then(|mut file| {
        if offset != 0 {
            file.seek(SeekFrom::Start(512 * offset)).map(|_| ())?;
        }

        (0..sectors).map(|_| file.write(&zeroed_sector).map(|_| ())).collect()
    })
}
