use {Chroot, Command, Config};
use disk::mount::{BIND, Mount};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

const APT_OPTIONS: &[&str] = &[
    "-o", "Acquire::cdrom::AutoDetect=0",
    "-o", "Acquire::cdrom::mount=/cdrom",
    "-o", "APT::CDROM::NoMount=1"
];

const BOOT_OPTIONS: &str = "quiet loglevel=0 systemd.show_status=false splash";

pub struct ChrootConfigure<'a> {
    chroot: Chroot<'a>
}

impl<'a> ChrootConfigure<'a> {
    pub fn new(chroot: Chroot<'a>) -> Self { Self { chroot }}

    pub fn unmount(&mut self, lazy: bool) -> io::Result<()> {
        self.chroot.unmount(lazy)
    }

    pub fn apt_install(&self, packages: &[&str]) -> io::Result<()> {
        info!("installing packages: {:?}", packages);
        self.chroot.command("apt-get", &cascade! {
            Vec::with_capacity(APT_OPTIONS.len() + packages.len() + 3);
            ..extend_from_slice(&["install", "-q", "-y"]);
            ..extend_from_slice(APT_OPTIONS);
            ..extend_from_slice(packages);
        }).run()
    }

    pub fn apt_remove(&self, packages: &[&str]) -> io::Result<()> {
        info!("removing packages: {:?}", packages);
        self.chroot.command("apt-get", &cascade! {
            Vec::with_capacity(packages.len() + 2);
            ..extend_from_slice(&["purge", "-y"]);
            ..extend_from_slice(packages);
        }).run()?;
        self.chroot.command("apt-get", &["autoremove", "-y", "--purge"]).run()
    }

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

    pub fn cdrom_add(&self) -> io::Result<()> {
        if Path::new("/cdrom").exists() {
            info!("adding apt-cdrom to /etc/apt/sources.list");
            let sources_list = self.chroot.path.join("etc/apt/sources.list");
            // TODO: fs::read_to_string()
            let mut buffer = String::new();
            {
                let mut file = fs::File::open(&sources_list)?;
                file.read_to_string(&mut buffer)?;
            }

            let mut file = fs::File::create(&sources_list)?;
            file.write_all(buffer.replace("deb cdrom:", "# deb cdrom:").as_bytes())
        } else {
            Ok(())
        }
    }

    pub fn cdrom_disable(&self) -> io::Result<()> {
        if Path::new("/cdrom").exists() {
            info!("disabling apt-cdrom from /etc/apt/sources.list");
            self.chroot.command("apt-cdrom", &cascade! {
                Vec::with_capacity(APT_OPTIONS.len() + 1);
                ..extend_from_slice(APT_OPTIONS);
                ..push("add");
            }).run()
        } else {
            Ok(())
        }
    }

    pub fn disable_nvidia(&self) {
        info!("attempting to disable nvidia-fallback.service");
        let args = &["disable", "nvidia-fallback.service"];
        if let Err(why) = self.chroot.command("systemctl", args).run() {
            warn!("disabling nvidia-fallback.service failed: {}", why);
        }
    }

    pub fn etc_cleanup(&self) -> io::Result<()> {
        let initramfs_post_update = self.chroot.path.join("etc/initramfs/post-update.d/");
        if initramfs_post_update.is_dir() {
            fs::remove_dir_all(&initramfs_post_update)
        } else {
            Ok(())
        }
    }

    pub fn generate_locale(&self, locale: &str) -> io::Result<()> {
        info!("generating locales via `locale-gen` and `update-locale`");
        self.chroot.command("locale-gen", &["--purge", locale]).run()?;
        self.chroot.command("update-locale", &["--reset", &["LANG=", locale].concat()]).run()
    }

    pub fn generate_machine_id(&self) -> io::Result<()> {
        info!("generating machine id via `dbus-uuidgen`");
        self.chroot.command("sh", &["-c", "dbus-uuidgen > /etc/hostname"]).run()
    }

    pub fn hostname(&self, hostname: &str) -> io::Result<()> {
        info!("setting hostname to {}", hostname);
        let mut file = fs::File::create(self.chroot.path.join("etc/hostname"))?;
        writeln!(&mut file, "{}", hostname)
    }

    pub fn hosts(&self, hostname: &str) -> io::Result<()> {
        info!("setting hosts file");
        let mut file = fs::File::create(self.chroot.path.join("etc/hosts"))?;
        writeln!(&mut file, r#"127.0.0.1	localhost
::1		localhost
127.0.1.1	{0}.localdomain	{0}"#, hostname)
    }

    pub fn keyboard_layout(&self, config: &Config) -> io::Result<()> {
        info!("configuring keyboard layout");
        // Ensure that localectl writes to the chroot, instead.
        let _etc_mount = Mount::new(&self.chroot.path.join("etc"), "/etc", "none", BIND, None)?;

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

        self.chroot.command("ln", &[
            "-s",
            "/etc/console-setup/cached_UTF-8_del.kmap.gz",
            "/etc/console-setup/cached.kmap.gz"
        ]).run()?;

        self.update_initramfs()
    }

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
        self.chroot.command("ln", &[
            "-sf",
            "../run/resolvconf/resolv.conf",
            "/etc/resolv.conf"
        ]).run()
    }

    pub fn recovery(
        &self,
        config: &Config,
        name: &str,
        root_uuid: &str,
        luks_uuid: &str
    ) -> io::Result<()> {
        let recovery_path = self.chroot.path.join("recovery");

        if recovery_path.exists() && Path::new("/boot/efi").is_dir() && Path::new("/cdrom").is_dir() {
            info!("creating recovery partition");

            let recovery_uuid = Command::new("findmnt")
                .args(&["-n", "-o", "UUID"])
                .arg(&recovery_path)
                .run_with_stdout()?;

            let efi_uuid = Command::new("findmnt")
                .args(&["-n", "-o", "UUID", "/boot/efi"])
                .run_with_stdout()?;

            let cdrom_uuid = Command::new("findmnt")
                .args(&["-n", "-o", "UUID", "/cdrom"])
                .run_with_stdout()?;

            let casper = ["casper-", &recovery_uuid].concat();
            let recovery = ["Recovery-", &recovery_uuid].concat();

            if recovery_uuid != cdrom_uuid {
                self.chroot.command("rsync", &[
                    "-KLav", "/cdrom/.disk", "/cdrom/dists", "/cdrom/pool", "/recovery"
                ]).run()?;

                self.chroot.command("rsync", &[
                    "-KLav", "/cdrom/casper/", &["/recovery/", &casper].concat()
                ]).run()?;
            }

            // Create recovery file.
            let recovery_data = format!(r#"HOSTNAME={}
LANG={}
KBD_LAYOUT={}
KBD_MODEL={}
KBD_VARIANT={}
EFI_UUID={}
RECOVERY_UUID={}
ROOT_UUID={}
LUKS_UUID={}
OEM_MODE=0
"#,
                config.hostname,
                config.lang,
                config.keyboard_layout,
                config.keyboard_model.as_ref().map(|x| x.as_str()).unwrap_or(""),
                config.keyboard_variant.as_ref().map(|x| x.as_str()).unwrap_or(""),
                efi_uuid,
                recovery_uuid,
                root_uuid,
                luks_uuid,
            );

            // Copy initrd and vmlinuz to EFI partition
            let recovery_path = self.chroot.path.join("recovery/recovery.conf");
            let mut recovery_file = fs::File::create(&recovery_path)?;
            recovery_file.write_all(recovery_data.as_bytes())?;

            // Create bootloader configuration
            let efi_recovery = ["boot/efi/EFI/", recovery.as_str()].concat();
            let efi_initrd = self.chroot.path.join([&efi_recovery, "/initrd.gz"].concat());
            let efi_vmlinuz = self.chroot.path.join([&efi_recovery, "/vmlinuz.efi"].concat());
            let casper_initrd = self.chroot.path.join(["recovery/", &casper, "/initrd.gz"].concat());
            let casper_vmlinuz = self.chroot.path.join(["recovery/", &casper, "/vmlinuz.efi"].concat());

            fs::create_dir_all(self.chroot.path.join(efi_recovery))?;
            io::copy(&mut fs::File::open(casper_initrd)?, &mut fs::File::create(efi_initrd)?)?;
            io::copy(&mut fs::File::open(casper_vmlinuz)?, &mut fs::File::create(efi_vmlinuz)?)?;

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
            let rec_entry_path = self.chroot.path.join(
                ["boot/efi/loader/entries/", recovery.as_str(), ".conf"].concat()
            );
            let mut rec_entry_file = fs::File::create(&rec_entry_path)?;
            rec_entry_file.write_all(rec_entry_data.as_bytes())?;
        }

        Ok(())
    }

    pub fn update_initramfs(&self) -> io::Result<()> {
        self.chroot.command("update-initramfs", &["-u"]).run()
    }
}
