use super::physical_volumes_to_deactivate;
use disk::{Mounts, Swaps};
use disk::external::{pvs, lvs, vgdeactivate, cryptsetup_close};
use disk::mount::{umount, swapoff};
use std::io;
use std::path::Path;

pub(crate) fn deactivate_devices<P: AsRef<Path>>(devices: &[P]) -> io::Result<()> {
    let mounts = Mounts::new().unwrap();
    let swaps = Swaps::new().unwrap();
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
