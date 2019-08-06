use libparted::Disk as PedDisk;
use std::io;

/// Removes the partition on a block device by its sector location.
pub fn remove_partition_by_sector(disk: &mut PedDisk, sector: u64) -> io::Result<()> {
    {
        let dev = unsafe { disk.get_device() };
        info!("removing partition at sector {} on {}", sector, dev.path().display());
    }

    disk.remove_partition_by_sector(sector as i64).map_err(|why| {
        io::Error::new(
            why.kind(),
            format!(
                "failed to remove partition at sector {} on {}: {}",
                sector,
                unsafe { disk.get_device().path().display() },
                why
            ),
        )
    })
}

/// Removes the partition on a block device by its number.
pub fn remove_partition_by_number(disk: &mut PedDisk, num: u32) -> io::Result<()> {
    {
        let dev = unsafe { disk.get_device() };
        info!("removing partition {} on {}", num, dev.path().display());
    }

    disk.remove_partition_by_number(num).map_err(|why| {
        io::Error::new(
            why.kind(),
            format!(
                "failed to remove partition {} on {}: {}",
                num,
                unsafe { disk.get_device().path().display() },
                why
            ),
        )
    })
}
