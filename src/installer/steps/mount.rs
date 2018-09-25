use disk::{Disks, FileSystemType};
use mnt::{Mount, Mounts};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

pub fn mount(disks: &Disks, chroot: &Path) -> io::Result<Mounts> {
    let targets = disks.get_partitions()
        .filter(|part| part.target.is_some() && part.filesystem.is_some());

    let mut mounts = Vec::new();

    // The mount path will actually consist of the target concatenated with the
    // root. NOTE: It is assumed that the target is an absolute path.
    let paths: BTreeMap<PathBuf, (PathBuf, &'static str)> = targets
        .map(|target| {
            // Path mangling commences here, since we need to concatenate an absolute
            // path onto another absolute path, and the standard library opts for
            // overwriting the original path when doing that.
            let target_mount: PathBuf = {
                // Ensure that the chroot path has the ending '/'.
                let chroot = chroot.as_os_str().as_bytes();
                let mut target_mount: Vec<u8> = if chroot[chroot.len() - 1] == b'/' {
                    chroot.to_owned()
                } else {
                    let mut temp = chroot.to_owned();
                    temp.push(b'/');
                    temp
                };

                // Cut the starting '/' from the target path if it exists.
                let target_path = target.target.as_ref().unwrap().as_os_str().as_bytes();
                let target_path = if ! target_path.is_empty() && target_path[0] == b'/' {
                    if target_path.len() > 1 { &target_path[1..] } else { b"" }
                } else {
                    target_path
                };

                // Append the target path to the chroot, and return it as a path type.
                target_mount.extend_from_slice(target_path);
                PathBuf::from(OsString::from_vec(target_mount))
            };

            let fs = match target.filesystem.unwrap() {
                FileSystemType::Fat16 | FileSystemType::Fat32 => "vfat",
                fs => fs.into(),
            };

            (target_mount, (target.device_path.clone(), fs))
        })
        .collect();

    // Each mount directory will be created and then mounted before progressing to
    // the next mount in the map. The BTreeMap that the mount targets were
    // collected into will ensure that mounts are created and mounted in
    // the correct order.
    for (target_mount, (device_path, filesystem)) in paths {
        if let Err(why) = fs::create_dir_all(&target_mount) {
            error!("unable to create '{}': {}", why, target_mount.display());
        }

        mounts.push(Mount::new(
            &device_path,
            &target_mount,
            filesystem,
            0,
            None,
        )?);
    }

    Ok(Mounts(mounts))
}
