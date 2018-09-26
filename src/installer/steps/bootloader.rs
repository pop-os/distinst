use {Config, MODIFY_BOOT_ORDER};
use libc;
use process::Chroot;
use disk::{Bootloader, Disks};
use os_release::OsRelease;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use super::mount_efivars;

pub fn bootloader<F: FnMut(i32)>(
    disks: &Disks,
    mount_dir: &Path,
    bootloader: Bootloader,
    config: &Config,
    iso_os_release: &OsRelease,
    mut callback: F,
) -> io::Result<()> {
    // Obtain the root device & partition, with an optional EFI device & partition.
    let ((root_dev, _root_part), boot_opt) = disks.get_base_partitions(bootloader);

    let mut efi_part_num = 0;

    let bootloader_dev = boot_opt.map_or(root_dev, |(dev, dev_part)| {
        efi_part_num = dev_part.number;
        dev
    });

    info!(
        "{}: installing bootloader for {:?}",
        bootloader_dev.display(),
        bootloader
    );

    {
        let efi_path = {
            let chroot = mount_dir.as_os_str().as_bytes();
            let mut target_mount: Vec<u8> = if chroot[chroot.len() - 1] == b'/' {
                chroot.to_owned()
            } else {
                let mut temp = chroot.to_owned();
                temp.push(b'/');
                temp
            };

            target_mount.extend_from_slice(b"boot/efi/");
            PathBuf::from(OsString::from_vec(target_mount))
        };

        // Also ensure that the /boot/efi directory is created.
        if bootloader == Bootloader::Efi && boot_opt.is_some() {
            fs::create_dir_all(&efi_path)?;
        }

        {
            let mut chroot = Chroot::new(mount_dir)?;
            let efivars_mount = mount_efivars(&mount_dir)?;

            match bootloader {
                Bootloader::Bios => {
                    chroot.command(
                        "grub-install",
                        &[
                            // Recreate device map
                            "--recheck".into(),
                            // Install for BIOS
                            "--target=i386-pc".into(),
                            // Install to the bootloader_dev device
                            bootloader_dev.to_str().unwrap().to_owned(),
                        ],
                    ).run()?;
                }
                Bootloader::Efi => {
                    let name = &iso_os_release.name;
                    if &iso_os_release.name == "Pop!_OS" {
                        chroot.command(
                            "bootctl",
                            &[
                                // Install systemd-boot
                                "install",
                                // Provide path to ESP
                                "--path=/boot/efi",
                                // Do not set EFI variables
                                "--no-variables",
                            ][..],
                        ).run()?;
                    } else {
                        chroot.command(
                            "/usr/bin/env",
                            &[
                                "bash",
                                "-c",
                                "echo GRUB_ENABLE_CRYPTODISK=y >> /etc/default/grub"
                            ]
                        ).run()?;

                        chroot.command(
                            "grub-install",
                            &[
                                "--target=x86_64-efi",
                                "--efi-directory=/boot/efi",
                                &format!("--boot-directory=/boot/efi/EFI/{}", name),
                                &format!("--bootloader={}", name),
                                "--recheck",
                            ]
                        ).run()?;

                        chroot.command(
                            "grub-mkconfig",
                            &[ "-o", &format!("/boot/efi/EFI/{}/grub/grub.cfg", name)]
                        ).run()?;

                        chroot.command(
                            "update-initramfs",
                            &["-c", "-k", "all"]
                        ).run()?;
                    }

                    if config.flags & MODIFY_BOOT_ORDER != 0 {
                        let efi_part_num = efi_part_num.to_string();
                        let loader = if &iso_os_release.name == "Pop!_OS" {
                            "\\EFI\\systemd\\systemd-bootx64.efi".into()
                        } else {
                            format!("\\EFI\\{}\\grubx64.efi", name)
                        };

                        let args: &[&OsStr] = &[
                            "--create".as_ref(),
                            "--disk".as_ref(),
                            bootloader_dev.as_ref(),
                            "--part".as_ref(),
                            efi_part_num.as_ref(),
                            "--write-signature".as_ref(),
                            "--label".as_ref(),
                            iso_os_release.pretty_name.as_ref(),
                            "--loader".as_ref(),
                            loader.as_ref()
                        ][..];

                        chroot.command("efibootmgr", args).run()?;
                    }
                }
            }

            // Sync to the disk before unmounting
            unsafe { libc::sync(); }

            drop(efivars_mount);
            chroot.unmount(false)?;
        }
    }

    callback(99);

    Ok(())
}
