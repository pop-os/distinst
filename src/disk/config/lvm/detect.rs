use std::fs::read_link;
use std::path::{Path, PathBuf};
use misc::{concat_osstr, device_maps, read_dirs};

/// The input shall contain physical device paths (ie: /dev/sda1), and the output
/// will contain a list of physical volumes (ie: /dev/mapper/cryptroot) that need
/// to be deactivated.
pub(crate) fn physical_volumes_to_deactivate<P: AsRef<Path>>(paths: &[P]) -> Vec<PathBuf> {
    info!("libdistinst: searching for device maps to deactivate");
    let mut discovered = Vec::new();

    device_maps(|pv| {
        info!(
            "libdistinst: checking if {} needs to be marked",
            pv.display()
        );

        if let Ok(path) = read_link(pv) {
            let slave_path = concat_osstr(&[
                "/sys/block/".as_ref(),
                path.file_name().expect("pv does not have file name"),
                "/slaves".as_ref(),
            ]);

            let _ = read_dirs(&slave_path, |slave| {
                let slave_path = slave.path();
                let slave_path = slave_path.file_name().expect("slave path does not have file name");
                if paths
                    .iter()
                    .any(|p| p.as_ref().file_name().expect("slave path does not have file name") == slave_path)
                {
                    info!("libdistinst: marking to deactivate {}", pv.display());
                    discovered.push(pv.to_path_buf());
                }
            });
        }
    });

    discovered
}
