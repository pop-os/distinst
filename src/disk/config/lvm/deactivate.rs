use super::physical_volumes_to_deactivate;
use disk::external::{cryptsetup_close, lvs, pvs, vgdeactivate};
use disk::mount::{swapoff, umount};
use disk::{MOUNTS, SWAPS};
use std::io;
use std::path::Path;

pub(crate) fn deactivate_devices<P: AsRef<Path>>(devices: &[P]) -> io::Result<()> {
    let mounts = MOUNTS.read().expect("failed to get mounts in deactivate_devices");
    let swaps = SWAPS.read().expect("failed to get swaps in deactivate_devices");
    let umount = move |vg: &str| -> io::Result<()> {
        for lv in lvs(vg)? {
            if let Some(mount) = mounts.get_mount_point(&lv) {
                info!(
                    "libdistinst: unmounting logical volume mounted at {}",
                    mount.display()
                );
                umount(&mount, false)?;
            } else if let Ok(lv) = lv.canonicalize() {
                if swaps.get_swapped(&lv) {
                    swapoff(&lv)?;
                }
            }
        }

        Ok(())
    };

    for pv in &physical_volumes_to_deactivate(devices) {
        let mut pvs = pvs()?;
        match pvs.remove(pv) {
            Some(Some(ref vg)) => umount(vg)
                .and_then(|_| vgdeactivate(vg))
                .and_then(|_| cryptsetup_close(pv))?,
            _ => cryptsetup_close(pv)?,
        }
    }

    Ok(())
}
