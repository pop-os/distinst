use chroot::Chroot;
use disks::{Bootloader, Disks};
use errors::IoContext;
use libc;
use os_release::OsRelease;
use std::{
    ffi::{OsStr, OsString},
    fs, io,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};
use Config;
use MODIFY_BOOT_ORDER;

use super::mount_efivars;

pub fn bootloader<F: FnMut(i32)>(
    disks: &Disks,
    mount_dir: &Path,
    bootloader: Bootloader,
    config: &Config,
    iso_os_release: &OsRelease,
    recovery: bool,
    mut callback: F,
) -> io::Result<()> {
    // Obtain the root device & partition, with an optional EFI device & partition.
    let ((root_dev, _root_part), boot_opt) = disks.get_base_partitions(bootloader);

    let mut efi_part_num = 0;

    let bootloader_dev = boot_opt.map_or(root_dev, |(dev, dev_part)| {
        efi_part_num = dev_part.number;
        dev
    });

    info!("{}: installing bootloader for {:?}", bootloader_dev.display(), bootloader);

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
            fs::create_dir_all(&efi_path)
                .with_context(|err| format!("failed to create efi directory: {}", err))?;
        }

        {
            let mut chroot = Chroot::new(mount_dir)?;
            let efivars_mount = mount_efivars(&mount_dir)?;
            let needs_boot_fix = needs_boot_fix(recovery);

            match bootloader {
                Bootloader::Bios => {
                    chroot
                        .command(
                            "grub-install",
                            &[
                                // Recreate device map
                                "--recheck".into(),
                                // Install for BIOS
                                "--target=i386-pc".into(),
                                // Install to the bootloader_dev device
                                bootloader_dev.to_str().unwrap().to_owned(),
                            ],
                        )
                        .run()?;
                }
                Bootloader::Efi => {
                    // Grub disallows whitespaces in the name.
                    let name = super::normalize_os_release_name(&iso_os_release.name);
                    if &name == "Pop!_OS" {
                        if !needs_boot_fix {
                            chroot
                                .command(
                                    "bootctl",
                                    &[
                                        // Install systemd-boot
                                        "install",
                                        // Provide path to ESP
                                        "--path=/boot/efi",
                                        // Do not set EFI variables
                                        "--no-variables",
                                    ][..],
                                )
                                .run()?;
                        }
                    } else {
                        chroot
                            .command(
                                "/usr/bin/env",
                                &[
                                    "bash",
                                    "-c",
                                    "echo GRUB_ENABLE_CRYPTODISK=y >> /etc/default/grub",
                                ],
                            )
                            .run()?;

                        chroot
                            .command(
                                "grub-install",
                                &[
                                    "--target=x86_64-efi",
                                    "--efi-directory=/boot/efi",
                                    &format!("--boot-directory=/boot/efi/EFI/{}", name),
                                    &format!("--bootloader={}", name),
                                    "--no-nvram",
                                    "--recheck",
                                ],
                            )
                            .run()?;

                        chroot
                            .command(
                                "grub-mkconfig",
                                &["-o", &format!("/boot/efi/EFI/{}/grub/grub.cfg", name)],
                            )
                            .run()?;

                        chroot.command("update-initramfs", &["-c", "-k", "all"]).run()?;
                    }

                    if config.flags & MODIFY_BOOT_ORDER != 0 && !needs_boot_fix {
                        let efi_part_num = efi_part_num.to_string();
                        let loader = if &name == "Pop!_OS" {
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
                            loader.as_ref(),
                        ][..];

                        chroot.command("efibootmgr", args).run()?;
                    }
                }
            }

            // Sync to the disk before unmounting
            unsafe {
                libc::sync();
            }

            drop(efivars_mount);
            chroot.unmount(false)?;
        }
    }

    callback(99);

    Ok(())
}

use envfile::EnvFile;
use sysfs_class::DmiId;

/// Signifies that a restart workaround is required
fn needs_boot_fix(recovery: bool) -> bool {
    (recovery || oem_mode()) && open_model()
}

/// True if the model is the darp6 or galp4
fn open_model() -> bool {
    DmiId::default()
        .product_version()
        .map(|name| {
            let name = name.trim();
            name == "darp6" || name == "galp4"
        })
        .unwrap_or(false)
}

/// True if the installer is in OEM mode.
fn oem_mode() -> bool {
    let recovery_path = Path::new("/cdrom/recovery.conf");
    if recovery_path.exists() {
        match EnvFile::new(recovery_path) {
            Ok(env) => {
                return env.get("OEM_MODE") == Some("1");
            }
            Err(why) => {
                error!("failed to read recovery config: {}", why);
            }
        }

    }

    false
}
