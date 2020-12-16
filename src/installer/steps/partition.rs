use crate::disks::{operations::FormatPartitions, Disks};
use crate::errors::IoContext;
use crate::external::{blockdev, pvs, vgactivate, vgdeactivate};
use itertools::Itertools;
use rayon::{self, prelude::*};
use std::{collections::BTreeMap, io, path::PathBuf, thread::sleep, time::Duration};

pub fn partition<F: FnMut(i32)>(disks: &mut Disks, mut callback: F) -> io::Result<()> {
    let (pvs_result, commit_result): (
        io::Result<BTreeMap<PathBuf, Option<String>>>,
        io::Result<()>,
    ) = rayon::join(
        || {
            // This collection of physical volumes and their optional volume groups
            // will be used to obtain a list of volume groups associated with our
            // modified partitions.
            pvs().with_context(|why| format!("failed to get PVS map: {}", why))
        },
        || {
            // Perform layout changes serially, due to libparted thread safety issues,
            // and collect a list of partitions to format which can be done in parallel.
            // Once partitions have been formatted in parallel, reload the disk configuration.
            let mut partitions_to_format = FormatPartitions(Vec::new());
            for disk in disks.get_physical_devices_mut() {
                info!("{}: Committing changes to disk", disk.path().display());
                if let Some(partitions) =
                    disk.commit().with_context(|why| format!("disk commit error: {}", why))?
                {
                    partitions_to_format.0.extend_from_slice(&partitions.0);
                }
            }

            partitions_to_format.format()?;

            disks.physical.iter_mut().map(|disk| disk.reload().map_err(io::Error::from)).collect()
        },
    );

    let pvs = commit_result.and(pvs_result)?;

    callback(25);

    // Utilizes the physical volume collection to generate a vector of volume
    // groups which we will need to deactivate pre-`blockdev`, and will be
    // reactivated post-`blockdev`.
    let vgs = disks
        .get_physical_partitions()
        .filter_map(|part| match pvs.get(&part.device_path) {
            Some(&Some(ref vg)) => Some(vg.clone()),
            _ => None,
        })
        .unique()
        .collect::<Vec<String>>();

    // Deactivate logical volumes so that blockdev will not fail.
    vgs.iter().map(|vg| vgdeactivate(vg)).collect::<io::Result<()>>()?;

    // Ensure that the logical volumes have had time to deactivate.
    sleep(Duration::from_secs(1));
    callback(50);

    // This is to ensure that everything's been written and the OS is ready to
    // proceed.
    disks.physical.par_iter().for_each(|disk| {
        let _ = blockdev(&disk.path(), &["--flushbufs", "--rereadpt"]);
    });

    // Give a bit of time to ensure that logical volumes can be re-activated.
    sleep(Duration::from_secs(1));
    callback(75);

    // Reactivate the logical volumes.
    vgs.iter().map(|vg| vgactivate(vg)).collect::<io::Result<()>>()?;

    let res = disks
        .commit_logical_partitions()
        .with_context(|why| format!("failed to commit logical partitions: {}", why));

    callback(100);
    res
}
