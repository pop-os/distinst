use chroot::{Chroot, Command};
use Config;
use misc;
use proc_mounts::MountList;
use partition_identity::PartitionID;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use sys_mount::*;
use timezones::Region;
use std::process::Stdio;

const APT_OPTIONS: &[&str] = &[
    "-o", "Acquire::cdrom::AutoDetect=0",
    "-o", "Acquire::cdrom::mount=/cdrom",
    "-o", "APT::CDROM::NoMount=1"
];

const BOOT_OPTIONS: &str = "quiet loglevel=0 systemd.show_status=false splash";

pub struct ChrootConfigurator<'a> {
    chroot: Chroot<'a>
}

impl<'a> ChrootConfigurator<'a> {
    pub fn new(chroot: Chroot<'a>) -> Self { Self { chroot }}

    /// Install the given packages if they are not already installed.
    pub fn apt_install(&self, packages: &[&str]) -> io::Result<()> {
        info!("installing packages: {:?}", packages);
        let mut command = self.chroot.command("apt-get", &cascade! {
            Vec::with_capacity(APT_OPTIONS.len() + packages.len() + 3);
            ..extend_from_slice(&["install", "-q", "-y"]);
            ..extend_from_slice(APT_OPTIONS);
            ..extend_from_slice(&packages);
        });
        
        command.stdout(Stdio::null());
        command.run()
    }

    /// Remove the given packages from the system, if they are installed.
    pub fn apt_remove(&self, packages: &[&str]) -> io::Result<()> {
        info!("removing packages: {:?}", packages);
        self.chroot.command("apt-get", &cascade! {
            Vec::with_capacity(packages.len() + 2);
            ..extend_from_slice(&["purge", "-y"]);
            ..extend_from_slice(packages);
        }).run()?;
        self.chroot.command("apt-get", &["autoremove", "-y", "--purge"]).run()
    }

    /// Configure the bootloader on the system.
    pub fn bootloader(&self) -> io::Result<()> {
        info!("configuring bootloader");
        let result = self.chroot.command("kernelstub", &[
            "--esp-path", "/boot/efi",
            "--kernel-path", "/vmlinuz",
            "--initrd-path", "/initrd.img",
            "--options", BOOT_OPTIONS,
            "--loader",
            "--manage-only",
            "--force-update",
            "--verbose"
        ]).run();

        match result {
            Ok(()) => Ok(()),
            // If kernelstub was not found, use grub instead.
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                let args: &[&str] = &[];
                self.chroot.command("update-grub", args).run()
            }
            Err(why) => Err(why),
        }
    }

    /// Add the apt repository on the image, so that packages may be installed from it.
    pub fn cdrom_add(&self) -> io::Result<()> {
        if Path::new("/cdrom").exists() {
            info!("adding apt-cdrom to /etc/apt/sources.list");
            self.chroot.command("apt-cdrom", &cascade! {
                Vec::with_capacity(APT_OPTIONS.len() + 1);
                ..extend_from_slice(APT_OPTIONS);
                ..push("add");
            }).run()
        } else {
            Ok(())
        }
    }

    /// Disable that repository, now that they system has been installed.
    pub fn cdrom_disable(&self) -> io::Result<()> {
        if Path::new("/cdrom").exists() {
            info!("disabling apt-cdrom from /etc/apt/sources.list");
            let path = self.chroot.path.join("etc/apt/sources.list");
            misc::sed(&path, "s/deb cdrom:/# deb cdrom:/g")
        } else {
            Ok(())
        }
    }

    /// Create a new user account.
    pub fn create_user(&self, user: &str, pass: Option<&str>, fullname: Option<&str>) -> io::Result<()> {
        let mut command = self.chroot.command("useradd", &["-m", "-G", "adm,sudo"]);
        if let Some(name) = fullname {
            command.args(&["-c", name, user]);
        } else {
            command.arg(user);
        };

        command.run()?;

        if let Some(pass) = pass {
            let pass = [pass, "\n", pass, "\n"].concat();
            self.chroot.command("passwd", &[user]).stdin_input(&pass).run()?;
        }

        Ok(())
    }

    /// Disable the nvidia fallback service.
    pub fn disable_nvidia(&self) {
        info!("attempting to disable nvidia-fallback.service");
        let args = &["disable", "nvidia-fallback.service"];
        if let Err(why) = self.chroot.command("systemctl", args).run() {
            warn!("disabling nvidia-fallback.service failed: {}", why);
        }
    }

    /// Remove files from /etc/ that may interfere with a reinstall.
    pub fn etc_cleanup(&self) -> io::Result<()> {
        let initramfs_post_update = self.chroot.path.join("etc/initramfs/post-update.d/");
        if initramfs_post_update.is_dir() {
            fs::remove_dir_all(&initramfs_post_update)
        } else {
            Ok(())
        }
    }

    /// Use locale-gen and update-locale to set the locale of the machine.
    pub fn generate_locale(&self, locale: &str) -> io::Result<()> {
        info!("generating locales via `locale-gen` and `update-locale`");
        self.chroot.command("locale-gen", &["--purge", locale]).run()?;
        self.chroot.command("update-locale", &["--reset", &["LANG=", locale].concat()]).run()
    }

    /// Generate a new machine ID for /var/lib/dbus/machine-id
    pub fn generate_machine_id(&self) -> io::Result<()> {
        info!("generating machine id via `dbus-uuidgen`");
        self.chroot.command("sh", &["-c", "dbus-uuidgen > /etc/machine-id"]).run()?;
        self.chroot.command("ln", &["-sf", "/etc/machine-id", "/var/lib/dbus/machine-id"]).run()
    }

    /// Set the hostname of the new install.
    pub fn hostname(&self, hostname: &str) -> io::Result<()> {
        info!("setting hostname to {}", hostname);
        let mut file = misc::create(&self.chroot.path.join("etc/hostname"))?;
        writeln!(&mut file, "{}", hostname)
    }

    /// Create a default hosts file for the new install.
    pub fn hosts(&self, hostname: &str) -> io::Result<()> {
        info!("setting hosts file");
        let mut file = misc::create(&self.chroot.path.join("etc/hosts"))?;
        writeln!(&mut file, r#"127.0.0.1	localhost
::1		localhost
127.0.1.1	{0}.localdomain	{0}"#, hostname)
    }

    /// Set the keyboard layout so that the layout will function, even within the decryption screen.
    pub fn keyboard_layout(&self, config: &Config) -> io::Result<()> {
        info!("configuring keyboard layout");
        // Ensure that localectl writes to the chroot, instead.
        let _etc_mount = Mount::new(&self.chroot.path.join("etc"), "/etc", "none", MountFlags::BIND, None)?
            .into_unmount_drop(UnmountFlags::DETACH);

        self.chroot.command("localectl", &[
            "set-x11-keymap",
            &config.keyboard_layout,
            config.keyboard_model.as_ref().map(|x| x.as_str()).unwrap_or(""),
            config.keyboard_variant.as_ref().map(|x| x.as_str()).unwrap_or(""),
        ]).run()?;

        self.chroot.command("/usr/bin/env", &[
            "-i", "SYSTEMCTL_SKIP_REDIRECT=_",
            "openvt", "--", "sh", "/etc/init.d/console-setup.sh", "reload"
        ]).run()?;

        let cached_file = self.chroot.path.join("etc/console-setup/cached.kmap.gz");
        if cached_file.exists () {
            fs::remove_file(cached_file)?;
        }

        self.chroot.command("ln", &[
            "-s",
            "/etc/console-setup/cached_UTF-8_del.kmap.gz",
            "/etc/console-setup/cached.kmap.gz"
        ]).run()
    }

    /// In case the kernel is located outside of the squashfs image, find it.
    pub fn kernel_copy(&self) -> io::Result<()> {
        let cdrom_kernel = Path::new("/cdrom/casper/vmlinuz");
        let chroot_kernel = self.chroot.path.join("vmlinuz");

        if cdrom_kernel.exists() && ! chroot_kernel.exists() {
            info!("copying kernel from /cdrom");
            self.chroot.command(
                "sh",
                &["-c", "cp /cdrom/casper/vmlinuz \"$(realpath /vmlinuz)\""]
            ).run()
        } else {
            Ok(())
        }
    }

    pub fn netresolve(&self) -> io::Result<()> {
        info!("creating /etc/resolv.conf");

        let resolvconf = "../run/systemd/resolve/stub-resolv.conf";
        self.chroot.command("ln", &["-sf", resolvconf, "/etc/resolv.conf"]).run()
    }

    pub fn recovery(
        &self,
        config: &Config,
        name: &str,
        root_uuid: &str,
        luks_uuid: &str
    ) -> io::Result<()> {
        info!("creating recovery partition");
        let recovery_path = self.chroot.path.join("recovery");
        let efi_path = self.chroot.path.join("boot/efi");

        let result = if recovery_path.exists() { 0 } else { 1 }
            | if efi_path.is_dir() { 0 } else { 2 }
            | if Path::new("/cdrom").is_dir() { 0 } else { 4 };

        if result != 0 {
            warn!("{}, therefore no recovery partition will be created", if result & 1 != 0 {
                format!("recovery at {} was not found", recovery_path.display())
            } else if result & 2 != 0 {
                format!("no EFI partition found at {}", efi_path.display())
            } else {
                "/cdrom was not found".into()
            });
            return Ok(());
        }

        let mounts = MountList::new()?;
        let recovery_mount = mounts.get_mount_by_dest(&recovery_path)
            .expect("/recovery is mount not associated with block device");

        let efi_mount = mounts.get_mount_by_dest(&efi_path)
            .expect("efi is mount not associated with block device");

        let recovery_partuuid = PartitionID::get_partuuid(&recovery_mount.source)
            .expect("/recovery does not have a PartUUID");
        let efi_partuuid = PartitionID::get_partuuid(&efi_mount.source)
            .expect("efi partiton does not have a PartUUID");

        let recovery_uuid = PartitionID::get_uuid(&recovery_mount.source)
            .or_else(|| PartitionID::get_uuid(&efi_mount.source))
            .expect("/recovery does not have a UUID");

        let cdrom_uuid = Command::new("findmnt")
            .args(&["-n", "-o", "UUID", "/cdrom"])
            .run_with_stdout()?;
        let cdrom_uuid = cdrom_uuid.trim();

        let casper = ["casper-", &recovery_uuid.id].concat();
        let recovery = ["Recovery-", &recovery_uuid.id].concat();
        if recovery_uuid.id != cdrom_uuid {
            self.chroot.command("rsync", &[
                "-KLavc", "/cdrom/.disk", "/cdrom/dists", "/cdrom/pool", "/recovery"
            ]).run()?;

            self.chroot.command("rsync", &[
                "-KLavc", "/cdrom/casper/", &["/recovery/", &casper].concat()
            ]).run()?;
        }

        // Create recovery file.
        let recovery_data = format!(r#"HOSTNAME={}
LANG={}
KBD_LAYOUT={}
KBD_MODEL={}
KBD_VARIANT={}
EFI_UUID=PARTUUID={}
RECOVERY_UUID=PARTUUID={}
ROOT_UUID={}
LUKS_UUID={}
OEM_MODE=0
"#,
            config.hostname,
            config.lang,
            config.keyboard_layout,
            config.keyboard_model.as_ref().map(|x| x.as_str()).unwrap_or(""),
            config.keyboard_variant.as_ref().map(|x| x.as_str()).unwrap_or(""),
            efi_partuuid.id,
            recovery_partuuid.id,
            root_uuid,
            luks_uuid,
        );

        // Copy initrd and vmlinuz to EFI partition
        let recovery_path = self.chroot.path.join("recovery/recovery.conf");
        let mut recovery_file = misc::create(&recovery_path)?;
        recovery_file.write_all(recovery_data.as_bytes())?;

        let efi_recovery = ["boot/efi/EFI/", recovery.as_str()].concat();
        let efi_initrd = self.chroot.path.join([&efi_recovery, "/initrd.gz"].concat());
        let efi_vmlinuz = self.chroot.path.join([&efi_recovery, "/vmlinuz.efi"].concat());

        fs::create_dir_all(self.chroot.path.join(efi_recovery))?;

        misc::cp("/cdrom/casper/initrd.gz", &efi_initrd)?;
        misc::cp("/cdrom/casper/vmlinuz.efi", &efi_vmlinuz)?;

        let rec_entry_data = format!(r#"title {0} recovery
linux /EFI/{1}/vmlinuz.efi
initrd /EFI/{1}/initrd.gz
options {2} boot=casper hostname=recovery userfullname=Recovery username=recovery live-media-path=/{3} noprompt
"#,
            name,
            recovery,
            BOOT_OPTIONS,
            casper
        );
        let loader_entries = self.chroot.path.join("boot/efi/loader/entries/");
        if ! loader_entries.exists() {
            fs::create_dir_all(&loader_entries)?;
        }

        let rec_entry_path = loader_entries.join([recovery.as_str(), ".conf"].concat());
        let mut rec_entry_file = misc::create(&rec_entry_path)?;
        rec_entry_file.write_all(rec_entry_data.as_bytes())?;
        Ok(())
    }

    pub fn timezone(&self, region: &Region) -> io::Result<()> {
        self.chroot.command("rm", &["/etc/timezone"]).run()?;

        let args: &[&str] = &[];
        self.chroot.command("ln", args)
            .arg(region.path())
            .arg("/etc/timezone")
            .run()
    }

    pub fn update_initramfs(&self) -> io::Result<()> {
        self.chroot.command("update-initramfs", &["-u"]).run().map_err(|why| io::Error::new(
            io::ErrorKind::Other,
            format!("failed to update initramfs: {}", why)
        ))
    }
}
