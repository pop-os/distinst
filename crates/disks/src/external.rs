//! A collection of external commands used throughout the program.

pub use crate::config::deactivate_devices;
pub use external_::*;
use misc;
use proc_mounts::{MountList, SwapList};
use std::{
    fs::Permissions,
    io::{self, Read, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
};
use sys_mount::*;
use tempdir::TempDir;
use crate::LuksEncryption;

fn remove_encrypted_device(device: &Path) -> io::Result<()> {
    let mounts = MountList::new().expect("failed to get mounts in deactivate_device_maps");
    let swaps = SwapList::new().expect("failed to get swaps in deactivate_device_maps");
    let umount = move |vg: &str| -> io::Result<()> {
        for lv in lvs(vg)? {
            if let Some(mount) = mounts.get_mount_by_source(&lv) {
                info!("libdistinst: unmounting logical volume mounted at {}", mount.dest.display());
                unmount(&mount.dest, UnmountFlags::empty())?;
            } else if let Ok(lv) = lv.canonicalize() {
                if swaps.get_swapped(&lv) {
                    swapoff(&lv)?;
                }
            }
        }

        Ok(())
    };

    let volume_map = pvs()?;
    let to_deactivate = physical_volumes_to_deactivate(&[device]);

    for pv in &to_deactivate {
        let dev = CloseBy::Path(&pv);
        match volume_map.get(pv) {
            Some(&Some(ref vg)) => umount(vg).and_then(|_| {
                info!("removing pre-existing LUKS + LVM volumes on {:?}", device);
                vgdeactivate(vg)
                    .and_then(|_| vgremove(vg))
                    .and_then(|_| pvremove(pv))
                    .and_then(|_| cryptsetup_close(dev))
            })?,
            Some(&None) => cryptsetup_close(dev)?,
            None => (),
        }
    }

    if let Some(ref vg) = volume_map.get(device).and_then(|x| x.as_ref()) {
        info!("removing pre-existing LVM volumes on {:?}", device);
        umount(vg).and_then(|_| vgremove(vg))?
    }

    Ok(())
}

/// Creates a LUKS partition from a physical partition. This could be either a LUKS on LVM
/// configuration, or a LVM on LUKS configurations.
pub fn cryptsetup_encrypt(device: &Path, enc: &LuksEncryption) -> io::Result<()> {
    remove_encrypted_device(device)?;

    info!("cryptsetup is encrypting {} with {:?}", device.display(), enc);

    match (enc.password.as_ref(), enc.keydata.as_ref()) {
        (Some(_password), Some(_keydata)) => unimplemented!(),
        (Some(password), None) => exec(
            "cryptsetup",
            Some(&append_newline(password.as_bytes())),
            None,
            &[
                "-s".into(),
                "512".into(),
                "luksFormat".into(),
                "--type".into(),
                "luks2".into(),
                device.into(),
            ],
        ),
        (None, Some(&(_, ref keydata))) => {
            let keydata = keydata.as_ref().expect("field should have been populated");
            let tmpfs = TempDir::new("distinst")?;
            let _mount = Mount::builder()
                .flags(MountFlags::BIND)
                .mount_autodrop(&keydata.0, tmpfs.path(), UnmountFlags::DETACH)?;

            let keypath = tmpfs.path().join(&enc.physical_volume);

            generate_keyfile(&keypath)?;
            info!("keypath exists: {}", keypath.is_file());

            exec(
                "cryptsetup",
                None,
                None,
                &[
                    "-s".into(),
                    "512".into(),
                    "luksFormat".into(),
                    "--type".into(),
                    "luks2".into(),
                    device.into(),
                    tmpfs.path().join(&enc.physical_volume).into(),
                ],
            )
        }
        (None, None) => unimplemented!(),
    }
}

/// Opens an encrypted partition and maps it to the pv name.
pub fn cryptsetup_open(device: &Path, enc: &LuksEncryption) -> io::Result<()> {
    deactivate_devices(&[device])?;
    let pv = &enc.physical_volume;
    info!("cryptsetup is opening {} with pv {} and {:?}", device.display(), pv, enc);
    match (enc.password.as_ref(), enc.keydata.as_ref()) {
        (Some(_password), Some(_keydata)) => unimplemented!(),
        (Some(password), None) => exec(
            "cryptsetup",
            Some(&append_newline(password.as_bytes())),
            None,
            &["open".into(), device.into(), pv.into()],
        ),
        (None, Some(&(_, ref keydata))) => {
            let keydata = keydata.as_ref().expect("field should have been populated");
            let tmpfs = TempDir::new("distinst")?;
            let _mount = Mount::builder()
                .flags(MountFlags::BIND)
                .mount_autodrop(&keydata.0, tmpfs.path(),UnmountFlags::DETACH)?;
            let keypath = tmpfs.path().join(&enc.physical_volume);
            info!("keypath exists: {}", keypath.is_file());

            exec(
                "cryptsetup",
                None,
                None,
                &["open".into(), device.into(), pv.into(), "--key-file".into(), keypath.into()],
            )
        }
        (None, None) => unimplemented!(),
    }
}

/// Append a newline to the input (used for the password)
fn append_newline(input: &[u8]) -> Vec<u8> {
    let mut input = input.to_owned();
    input.reserve_exact(1);
    input.push(b'\n');
    input
}

/// Generates a new keyfile by reading 512 bytes from "/dev/urandom".
fn generate_keyfile(path: &Path) -> io::Result<()> {
    info!("generating keyfile at {}", path.display());
    // Generate the key in memory from /dev/urandom.
    let mut key = [0u8; 512];
    let mut urandom = misc::open("/dev/urandom")?;
    urandom.read_exact(&mut key)?;

    // Open the keyfile and write the key, ensuring it is readable only to root.
    let mut keyfile = misc::create(path)?;
    keyfile.set_permissions(Permissions::from_mode(0o0400))?;
    keyfile.write_all(&key)?;
    keyfile.sync_all()
}
