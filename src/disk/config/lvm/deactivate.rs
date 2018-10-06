use super::physical_volumes_to_deactivate;
use process::external::{cryptsetup_close, lvs, pvs, vgdeactivate, CloseBy};
use mnt::{swapoff, MOUNTS, SWAPS};
use std::io;
use std::path::Path;
use sys_mount::{unmount, UnmountFlags};

pub(crate) fn deactivate_devices<P: AsRef<Path>>(devices: &[P]) -> io::Result<()> {
    let mounts = MOUNTS.read().expect("failed to get mounts in deactivate_devices");
    let swaps = SWAPS.read().expect("failed to get swaps in deactivate_devices");
    let umount = move |vg: &str| -> io::Result<()> {
        for lv in lvs(vg)? {
            if let Some(mount) = mounts.get_mount_point(&lv) {
                info!(
                    "unmounting logical volume mounted at {}",
                    mount.display()
                );
                unmount(&mount, UnmountFlags::empty())?;
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
        let device = CloseBy::Path(&pv);
        match pvs.remove(pv) {
            Some(Some(ref vg)) => umount(vg)
                .and_then(|_| vgdeactivate(vg))
                .and_then(|_| cryptsetup_close(device))?,
            _ => cryptsetup_close(device)?,
        }
    }

    Ok(())
}
