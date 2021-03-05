use super::*;
use misc::{concat_osstr, device_maps, read_dirs};
use proc_mounts::{MOUNTS, SWAPS};
use rand::{self, distributions::Alphanumeric, Rng};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::read_link,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use sys_mount::{swapoff, unmount, UnmountFlags};

pub fn deactivate_devices<P: AsRef<Path>>(devices: &[P]) -> io::Result<()> {
    let mounts = MOUNTS.read().expect("failed to get mounts in deactivate_devices");
    let swaps = SWAPS.read().expect("failed to get swaps in deactivate_devices");
    let umount = move |vg: &str| -> io::Result<()> {
        for lv in lvs(vg)? {
            if let Some(mount) = mounts.get_mount_by_source(&lv) {
                info!("unmounting logical volume mounted at {}", mount.dest.display());
                unmount(&mount.dest, UnmountFlags::empty())?;
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
            Some(Some(ref vg)) => {
                umount(vg).and_then(|_| vgdeactivate(vg)).and_then(|_| cryptsetup_close(device))?
            }
            _ => cryptsetup_close(device)?,
        }
    }

    Ok(())
}

/// The input shall contain physical device paths (ie: /dev/sda1), and the output
/// will contain a list of physical volumes (ie: /dev/mapper/cryptroot) that need
/// to be deactivated.
pub fn physical_volumes_to_deactivate<P: AsRef<Path>>(paths: &[P]) -> Vec<PathBuf> {
    info!("searching for device maps to deactivate");
    let mut discovered = Vec::new();

    device_maps(|pv| {
        if let Ok(path) = read_link(pv) {
            let slave_path = concat_osstr(&[
                "/sys/block/".as_ref(),
                path.file_name().expect("pv does not have file name"),
                "/slaves".as_ref(),
            ]);

            let _ = read_dirs(&slave_path, |slave| {
                let slave_path = slave.path();
                let slave_path =
                    slave_path.file_name().expect("slave path does not have file name");
                if paths.iter().any(|p| {
                    p.as_ref().file_name().expect("slave path does not have file name")
                        == slave_path
                }) {
                    discovered.push(pv.to_path_buf());
                }
            });
        }
    });

    discovered
}

/// Get a vector of logical devices.
pub fn dmlist() -> io::Result<Vec<String>> {
    let mut current_line = String::with_capacity(64);
    let mut output = Vec::new();

    let mut reader = BufReader::new(
        Command::new("dmsetup")
            .arg("ls")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .expect("failed to execute dmsetup command"),
    );

    // Parse the output of `dmsetup ls`, only taking the first field from each line.
    while reader.read_line(&mut current_line)? != 0 {
        if let Some(dm) = current_line.split_whitespace().next() {
            output.push(dm.into());
        }

        current_line.clear();
    }

    // Also add lvm volume groups from `vgdisplay`, which `dmsetup ls` does not list.
    output.extend_from_slice(&vgdisplay()?);

    Ok(output)
}

/// Generate a unique device map ID, to ensure no collisions between dm blocks.
pub fn generate_unique_id(prefix: &str, exclude_hashes: &[u64]) -> io::Result<String> {
    let dmlist = dmlist()?;
    let check_uniqueness = |id: &str, exclude: &[u64]| -> bool {
        !dmlist.iter().any(|x| x.as_str() == id) && !exclude.contains(&::misc::hasher(&id))
    };

    if check_uniqueness(prefix, exclude_hashes) {
        return Ok(prefix.into());
    }

    loop {
        let id: String = rand::thread_rng().sample_iter(&Alphanumeric).take(5).collect();
        let id = [prefix, "_", &id].concat();
        if !check_uniqueness(&id, exclude_hashes) {
            continue;
        }
        return Ok(id);
    }
}

/// Used to create a logical volume on a volume group.
pub fn lvcreate(group: &str, name: &str, size: Option<u64>) -> io::Result<()> {
    exec(
        "lvcreate",
        None,
        None,
        &size.map_or(
            ["-y".into(), "-l".into(), "100%FREE".into(), group.into(), "-n".into(), name.into()],
            |size| {
                [
                    "-y".into(),
                    "-L".into(),
                    mebibytes(size).into(),
                    group.into(),
                    "-n".into(),
                    name.into(),
                ]
            },
        ),
    )
}

/// Remove the logical volume, `name`, from the volume group, `group`.
pub fn lvremove(group: &str, name: &str) -> io::Result<()> {
    exec("lvremove", None, None, &["-y".into(), ["/dev/mapper/", group, "-", name].concat().into()])
}

/// Obtains a list of logical volumes associated with the given volume group.
pub fn lvs(vg: &str) -> io::Result<Vec<PathBuf>> {
    info!("obtaining logical volumes on {}", vg);
    let mut current_line = String::with_capacity(128);
    let mut output = Vec::new();

    let mut reader = BufReader::new(
        Command::new("lvs")
            .arg(vg)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .expect("failed to execute lvs command"),
    );

    // Skip the first line of output
    let _ = reader.read_line(&mut current_line);
    current_line.clear();

    while reader.read_line(&mut current_line)? != 0 {
        {
            let line = &current_line[2..];
            if let Some(pos) = line.find(' ') {
                let dev = PathBuf::from(
                    [
                        "/dev/mapper/",
                        &vg.replace("-", "--"),
                        "-",
                        &(&line[..pos].replace("-", "--")),
                    ]
                    .concat(),
                );

                output.push(dev);
            }
        }

        current_line.clear();
    }

    Ok(output)
}

/// Used to create a physical volume on a LUKS partition.
pub fn pvcreate<P: AsRef<Path>>(device: P) -> io::Result<()> {
    exec("pvcreate", None, None, &["-ffy".into(), device.as_ref().into()])
}

/// Obtains a map of physical volume paths and their optionally-assigned volume
/// groups.
pub fn pvs() -> io::Result<BTreeMap<PathBuf, Option<String>>> {
    info!("obtaining list of physical volumes");
    let mut current_line = String::with_capacity(64);
    let mut output = BTreeMap::new();

    let mut reader = BufReader::new(
        Command::new("pvs")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .expect("failed to execute pvs command"),
    );

    // Skip the first line of output
    let _ = reader.read_line(&mut current_line);
    current_line.clear();

    while reader.read_line(&mut current_line)? != 0 {
        {
            let mut fields = current_line[2..].split_whitespace();
            fields.next().map(|pv| {
                fields.next().map(|vg| {
                    eprintln!("Found PV {}: VG {}", pv, vg);
                    output.insert(
                        PathBuf::from(pv),
                        if vg.is_empty() || vg == "lvm2" { None } else { Some(vg.into()) },
                    )
                })
            });
        }

        current_line.clear();
    }

    Ok(output)
}

/// Deactivates all logical volumes in the supplied volume group
pub fn vgactivate(volume_group: &str) -> io::Result<()> {
    info!("activating '{}'", volume_group);
    let args = &["-ffyay".into(), volume_group.into()];
    exec("vgchange", None, None, args)
}

/// Used to create a volume group from one or more physical volumes.
pub fn vgcreate<I: Iterator<Item = S>, S: AsRef<OsStr>>(group: &str, devices: I) -> io::Result<()> {
    exec("vgcreate", None, None, &{
        let mut args = Vec::with_capacity(16);
        args.push("-ffy".into());
        args.push(group.into());
        args.extend(devices.map(|x| x.as_ref().into()));
        args
    })
}

/// Deactivates all logical volumes in the supplied volume group
pub fn vgdeactivate(volume_group: &str) -> io::Result<()> {
    info!("deactivating '{}'", volume_group);
    let args = &["-ffyan".into(), volume_group.into()];
    exec("vgchange", None, None, args)
}

/// Get a list of all volume groups.
fn vgdisplay() -> io::Result<Vec<String>> {
    let mut current_line = String::with_capacity(64);
    let mut output = Vec::new();

    let mut reader = BufReader::new(
        Command::new("vgdisplay")
            .arg("-s")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .stdout
            .expect("failed to execute vgdisplay command"),
    );

    while reader.read_line(&mut current_line)? != 0 {
        if let Some(dm) = current_line.split_whitespace().next() {
            output.push(dm[1..dm.len() - 1].into());
        }

        current_line.clear();
    }

    Ok(output)
}

/// Removes the given volume group from the system.
pub fn vgremove(group: &str) -> io::Result<()> {
    exec("vgremove", None, None, &["-ffy".into(), group.into()])
}

/// Removes the physical volume from the system.
pub fn pvremove(physical_volume: &Path) -> io::Result<()> {
    let args = &["-ffy".into(), physical_volume.into()];
    exec("pvremove", None, None, args)
}
