use crate::disks::*;
use crate::misc;
use rayon;
use std::{
    io::{self, BufRead},
    path::{Path, PathBuf},
};
use crate::Config;

pub fn initialize<F: FnMut(i32)>(
    disks: &mut Disks,
    config: &Config,
    mut callback: F,
) -> io::Result<(PathBuf, Vec<String>)> {
    info!("Initializing");

    let fetch_squashfs = || match Path::new(&config.squashfs).canonicalize() {
        Ok(squashfs) => {
            if squashfs.exists() {
                info!("config.squashfs: found at {}", squashfs.display());
                Ok(squashfs)
            } else {
                error!("config.squashfs: supplied file does not exist");
                Err(io::Error::new(io::ErrorKind::NotFound, "invalid squashfs path"))
            }
        }
        Err(err) => {
            error!("config.squashfs: {}", err);
            Err(err)
        }
    };

    let fetch_packages = || {
        let mut remove_pkgs = Vec::new();
        {
            let file = match misc::open(&config.remove) {
                Ok(file) => file,
                Err(err) => {
                    error!("config.remove: {}", err);
                    return Err(err);
                }
            };

            // Collects the packages that are to be removed from the install.
            for line_res in io::BufReader::new(file).lines() {
                match line_res {
                    // Only add package if it is not contained within lang_packs.
                    Ok(line) => remove_pkgs.push(line),
                    Err(err) => {
                        error!("config.remove: {}", err);
                        return Err(err);
                    }
                }
            }
        }

        Ok(remove_pkgs)
    };

    let verify_disks = |disks: &Disks| {
        disks.verify_keyfile_paths()?;
        Ok(())
    };

    let mut res_a = Ok(());
    let mut res_b = Ok(Vec::new());
    let mut res_c = Ok(());
    let mut res_d = Ok(PathBuf::new());

    rayon::scope(|s| {
        s.spawn(|_| {
            // Deactivate any open logical volumes & close any encrypted partitions.
            if let Err(why) = disks.deactivate_device_maps() {
                error!("device map deactivation error: {}", why);
                res_a = Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("device map deactivation error: {}", why),
                ));
                return;
            }

            // Unmount any mounted devices.
            if let Err(why) = disks.unmount_devices() {
                error!("device unmount error: {}", why);
                res_a = Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("device unmount error: {}", why),
                ));
                return;
            }

            res_a = Ok(());
        });

        s.spawn(|_| res_b = fetch_packages());
        s.spawn(|_| res_c = verify_disks(disks));
        s.spawn(|_| res_d = fetch_squashfs());
    });

    let (remove_pkgs, squashfs) =
        res_a.and(res_c).and(res_b).and_then(|pkgs| res_d.map(|squashfs| (pkgs, squashfs)))?;

    let unmount =
        disks.physical.iter().map(|disk| !disk.contains_mount("/", disks)).collect::<Vec<bool>>();

    disks
        .physical
        .iter_mut()
        .zip(unmount.into_iter())
        .filter(|&(_, unmount)| unmount)
        .map(|(disk, _)| {
            if let Err(why) = disk.unmount_all_partitions_with_target() {
                error!("unable to unmount partitions");
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("{:?}: {}", why.0, why.1),
                ));
            }

            Ok(())
        })
        .collect::<io::Result<()>>()?;

    callback(100);

    Ok((squashfs, remove_pkgs))
}
