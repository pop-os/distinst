use std::fs::{read_dir, read_link, DirEntry};
use std::io;
use std::path::{Path, PathBuf};

/// The input shall contain physical device paths (ie: /dev/sda1), and the output
/// will contain a list of physical volumes (ie: /dev/mapper/cryptroot) that need
/// to be deactivated.
pub(crate) fn physical_volumes_to_deactivate<P: AsRef<Path>>(paths: &[P]) -> Vec<PathBuf> {
    info!("libdistinst: searching for device maps to deactivate");
    let mut discovered = Vec::new();
    use std::ffi::{OsStr, OsString};

    device_maps(|pv| {
        info!(
            "libdistinst: checking if {} needs to be marked",
            pv.display()
        );

        if let Ok(path) = read_link(pv) {
            // NOTE: It would be nice if Rust supported &[&OsStr] -> OsString concat().
            //       This block wouldn't be needed if you could do that...
            let slave_path = {
                let slave_components: [&OsStr; 3] = [
                    "/sys/block/".as_ref(),
                    path.file_name().unwrap(),
                    "/slaves".as_ref(),
                ];

                let mut slave_path = OsString::with_capacity(
                    slave_components.iter().fold(0, |acc, c| acc + c.len()),
                );

                slave_components
                    .iter()
                    .for_each(|comp| slave_path.push(comp));
                slave_path
            };

            let _ = read_dirs(&slave_path, |slave| {
                let slave_path = slave.path();
                let slave_path = slave_path.file_name().unwrap();
                if paths
                    .iter()
                    .any(|p| p.as_ref().file_name().unwrap() == slave_path)
                {
                    info!("libdistinst: marking to deactivate {}", pv.display());
                    discovered.push(pv.to_path_buf());
                }
            });
        }
    });

    discovered
}

fn device_maps<F: FnMut(&Path)>(mut action: F) {
    read_dirs("/dev/mapper", |pv| action(&pv.path())).unwrap()
}

fn read_dirs<P: AsRef<Path>, F: FnMut(DirEntry)>(path: P, mut action: F) -> io::Result<()> {
    for entry in read_dir(path.as_ref())? {
        match entry {
            Ok(entry) => action(entry),
            Err(_) => continue,
        }
    }

    Ok(())
}
