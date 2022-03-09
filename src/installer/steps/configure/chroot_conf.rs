use crate::chroot::{Chroot, Command};
use crate::errors::{IoContext, IntoIoResult};
use crate::misc;
use partition_identity::PartitionID;
use proc_mounts::MountList;
use std::{
    fs,
    io::{self, Write},
    path::Path,
    process::Stdio,
};
use sys_mount::*;
use crate::timezones::Region;
use crate::Config;

const APT_OPTIONS: &[&str] = &[
    "-o",
    "Acquire::cdrom::AutoDetect=0",
    "-o",
    "Acquire::cdrom::mount=/cdrom",
    "-o",
    "APT::CDROM::NoMount=1",
];

// For a clean boot by default, we hide all output and use plymouth
const BOOT_OPTIONS: &str = "quiet loglevel=0 systemd.show_status=false splash";

// For a reliable boot when using recovery, we show all output and do not use plymouth
const RECOVERY_BOOT_OPTIONS: &str = "";

pub struct ChrootConfigurator<'a> {
    chroot: Chroot<'a>,
}

impl<'a> ChrootConfigurator<'a> {
    pub fn new(chroot: Chroot<'a>) -> Self { Self { chroot } }

    /// Install the given packages if they are not already installed.
    pub fn apt_install(&self, packages: &[&str]) -> io::Result<()> {
        info!("installing packages: {:?}", packages);
        let mut command = self.chroot.command(
            "apt-get",
            &cascade! {
                Vec::with_capacity(APT_OPTIONS.len() + packages.len() + 3);
                ..extend_from_slice(&["install", "-q", "-y"]);
                ..extend_from_slice(APT_OPTIONS);
                ..extend_from_slice(&packages);
            },
        );

        command.stdout(Stdio::null());
        command.run()
    }

    /// Remove the given packages from the system, if they are installed.
    pub fn apt_remove(&self, packages: &[&str]) -> io::Result<()> {
        info!("removing packages: {:?}", packages);
        self.chroot
            .command(
                "apt-get",
                &cascade! {
                    Vec::with_capacity(packages.len() + 2);
                    ..extend_from_slice(&["purge", "-y"]);
                    ..extend_from_slice(packages);
                },
            )
            .run()?;
        self.chroot.command("apt-get", &["autoremove", "-y", "--purge"]).run()
    }

    /// Configure the bootloader on the system.
    pub fn bootloader(&self, options: Option<&str>) -> io::Result<()> {
        info!("configuring bootloader");
        let result = self
            .chroot
            .command(
                "kernelstub",
                &[
                    "--esp-path",
                    "/boot/efi",
                    "--add-options",
                    BOOT_OPTIONS,
                    "--loader",
                    "--manage-only",
                    "--force-update",
                    "--verbose",
                ],
            )
            .run();

        match result {
            Ok(()) => {
                if let Some(options) = options {
                    self.chroot.command("kernelstub", &["-a", options]).run()?;
                }

                Ok(())
            },
            // If kernelstub was not found, use grub instead.
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                // Update the grub config to define the rootflags option.
                if let Some(options) = options {
                    let path = self.chroot.path.join("etc/default/grub");

                    let mut config = std::fs::read_to_string(&path)
                        .with_context(|err| format!("failed to read grub config: {}", err))?;

                    config = config.replace("GRUB_CMDLINE_LINUX=\"", &["GRUB_CMDLINE_LINUX=\"", options].concat());

                    std::fs::write(&path, config.as_bytes())
                        .with_context(|err| format!("failed to update grub config: {}", err))?;
                }

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
            self.chroot
                .command(
                    "apt-cdrom",
                    &cascade! {
                        Vec::with_capacity(APT_OPTIONS.len() + 1);
                        ..extend_from_slice(APT_OPTIONS);
                        ..push("add");
                    },
                )
                .run()
        } else {
            Ok(())
        }
    }

    pub fn install_drivers(&self, install: bool) -> io::Result<()> {
        let mut packages = Vec::new();
        if install {
            info!("finding drivers for hardware");
            let args: &[&str] = &["list"];
            let output = self.chroot.command("ubuntu-drivers", args).run_with_stdout()?;

            for result in output.lines().map(|line| line.split(",").nth(0)) {
                match result {
                    Some(package) => packages.push(package),
                    None => continue,
                }
            }

            info!("installing drivers: {:?}", packages);
            let mut command = self.chroot.command(
                "apt-get",
                &cascade! {
                    Vec::with_capacity(APT_OPTIONS.len() + packages.len() + 3);
                    ..extend_from_slice(&["install", "-q", "-y"]);
                    ..extend_from_slice(APT_OPTIONS);
                    ..extend(packages);
                },
            );

            command.stdout(Stdio::null());
            command.run()
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
    pub fn create_user(
        &self,
        user: &str,
        pass: Option<&str>,
        fullname: Option<&str>,
        profile_icon: Option<&str>,
    ) -> io::Result<()> {
        // Add the user to the system.
        {
            const DEFAULT_USERADD_FLAGS: &[&str] = &[
                "-m",
                "-G", "adm,sudo,lpadmin",
                "-s", "/bin/bash"
            ];

            let mut command = self.chroot.command("useradd", DEFAULT_USERADD_FLAGS);

            if let Some(name) = fullname {
                command.args(&["-c", name]);
            }

            command.arg(user).run()?;
        }

        // Set the password for the newly-created user.
        if let Some(pass) = pass {
            let pass = &[pass, "\n", pass, "\n"].concat();
            self.chroot.command("passwd", &[user]).stdin_input(pass).run()?;
        }

        // Copy the profile icon to `/var/lib/AccountsService/icons/{user}` and assign that in
        // the config file at `/var/lib/AccountsService/users/{user}`.
        if let Some(path) = profile_icon {
            let mut dest = self.chroot.path.join(&["var/lib/AccountsService/icons/", user].concat());

            if fs::copy(&path, &dest).is_err() {
                let _ = fs::remove_file(&dest);
                return Ok(());
            }

            dest = self.chroot.path.join(&["var/lib/AccountsService/users/", user].concat());

            if fs::write(&dest, fomat!(
                "[User]\n"
                "Icon=/var/lib/AccountsService/icons/" (user) "\n"
                "SystemAccount=false\n"
            )).is_err() {
                let _ = fs::remove_file(&dest);
            }
        }

        Ok(())
    }

    /// Disable the nvidia fallback service.
    pub fn disable_nvidia_fallback(&self) {
        info!("attempting to disable nvidia-fallback.service");
        let args = &["disable", "nvidia-fallback.service"];
        if let Err(why) = self.chroot.command("systemctl", args).run() {
            warn!("disabling nvidia-fallback.service failed: {}", why);
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
        let hostfile = self.chroot.path.join("etc/hostname");
        let mut file = misc::create(&hostfile)?;
        writeln!(&mut file, "{}", hostname)
            .with_context(|err| format!("failed to write hostname to {:?}: {}", hostfile, err))
    }

    /// Create a default hosts file for the new install.
    pub fn hosts(&self, hostname: &str) -> io::Result<()> {
        info!("setting hosts file");
        let hosts = self.chroot.path.join("etc/hosts");
        let mut file = misc::create(&hosts)?;
        writeln!(
            &mut file,
            r#"127.0.0.1	localhost
::1		localhost
127.0.1.1	{0}.localdomain	{0}"#,
            hostname
        )
        .with_context(|err| format!("failed to write hosts to {:?}: {}", hosts, err))
    }

    pub fn initramfs_disable(&self) -> io::Result<()> {
        info!("symlinking update-initramfs to true for duration of initial setup");

        self.chroot.command("sh", &["-c", "mv /usr/sbin/update-initramfs /usr/sbin/update-initramfs.bak"])
            .run()
            .with_context(|err| format!("failed to migrate `update-initramfs`: {}", err))?;

        self.chroot.command("sh", &["-c", "ln -s /usr/bin/true /usr/sbin/update-initramfs"])
            .run()
            .with_context(|err| format!("failed to link `true` to `update-initramfs`: {}", err))
    }

    pub fn initramfs_reenable(&self) -> io::Result<()> {
        info!("re-enabling update-initramfs");
        self.chroot.command("sh", &["-c", "rm /usr/sbin/update-initramfs"])
            .run()
            .with_context(|err| format!("failed to remove update-initramfs symlink: {}", err))?;

        self.chroot.command("sh", &["-c", "mv /usr/sbin/update-initramfs.bak /usr/sbin/update-initramfs"])
            .run()
            .with_context(|err| format!("failed to restore backup of update-initramfs: {}", err))
    }

    /// Set the keyboard layout so that the layout will function, even within the decryption screen.
    pub fn keyboard_layout(&self, config: &Config) -> io::Result<()> {
        info!("configuring keyboard layout");

        // Ensure that localectl writes to the chroot, instead.
        let source = self.chroot.path.join("etc");

        let _etc_mount =
            Mount::builder()
                .flags(MountFlags::BIND)
                .fstype("none")
                .mount_autodrop(source, "/etc", UnmountFlags::DETACH)?;

        self.chroot
            .command(
                "localectl",
                &[
                    "set-x11-keymap",
                    &config.keyboard_layout,
                    config.keyboard_model.as_deref().unwrap_or(""),
                    config.keyboard_variant.as_deref().unwrap_or(""),
                ],
            )
            .run()?;

        self.chroot
            .command(
                "/usr/bin/env",
                &[
                    "-i",
                    "SYSTEMCTL_SKIP_REDIRECT=_",
                    "openvt",
                    "--",
                    "sh",
                    "/etc/init.d/console-setup.sh",
                    "reload",
                ],
            )
            .run()?;

        let cached_file = self.chroot.path.join("etc/console-setup/cached.kmap.gz");
        if cached_file.exists() {
            fs::remove_file(cached_file)
                .with_context(|err| format!("failed to remove console-setup cache: {}", err))?;
        }

        self.chroot
            .command(
                "ln",
                &[
                    "-s",
                    "/etc/console-setup/cached_UTF-8_del.kmap.gz",
                    "/etc/console-setup/cached.kmap.gz",
                ],
            )
            .run()
    }

    /// In case the kernel is located outside of the squashfs image, find it.
    pub fn kernel_copy(&self) -> io::Result<()> {
        let cdrom_kernel = Path::new("/cdrom/casper/vmlinuz");
        let chroot_kernel = self.chroot.path.join("vmlinuz");

        if cdrom_kernel.exists() && !chroot_kernel.exists() {
            info!("copying kernel from /cdrom");
            self.chroot
                .command("sh", &["-c", "cp /cdrom/casper/vmlinuz \"$(realpath /vmlinuz)\""])
                .run()
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
        luks_uuid: &str,
    ) -> io::Result<()> {
        info!("creating recovery partition");
        let recovery_path = self.chroot.path.join("recovery");
        let efi_path = self.chroot.path.join("boot/efi");

        let result = if recovery_path.exists() { 0 } else { 1 }
            | if efi_path.is_dir() { 0 } else { 2 }
            | if Path::new("/cdrom").is_dir() { 0 } else { 4 };

        if result != 0 {
            warn!(
                "{}, therefore no recovery partition will be created",
                if result & 1 != 0 {
                    format!("recovery at {} was not found", recovery_path.display())
                } else if result & 2 != 0 {
                    format!("no EFI partition found at {}", efi_path.display())
                } else {
                    "/cdrom was not found".into()
                }
            );
            return Ok(());
        }

        let mounts = MountList::new()?;
        let recovery_mount = mounts
            .get_mount_by_dest(&recovery_path)
            .into_io_result(|| "/recovery is mount not associated with block device")?;

        let efi_mount = mounts
            .get_mount_by_dest(&efi_path)
            .into_io_result(|| "efi is mount not associated with block device")?;

        let efi_partuuid = PartitionID::get_partuuid(&efi_mount.source)
            .into_io_result(|| "efi partiton does not have a PartUUID")?;

        let recovery_partuuid = PartitionID::get_partuuid(&recovery_mount.source)
            .into_io_result(|| "/recovery does not have a PartUUID")?;

        let recovery_uuid = PartitionID::get_uuid(&recovery_mount.source)
            .or_else(|| PartitionID::get_uuid(&efi_mount.source))
            .into_io_result(|| "/recovery does not have a UUID")?;

        let cdrom_uuid =
            Command::new("findmnt").args(&["-n", "-o", "UUID", "/cdrom"]).run_with_stdout()?;
        let cdrom_uuid = cdrom_uuid.trim();

        // If we are installing from the recovery partition, then we can skip this step.
        if recovery_uuid.id == cdrom_uuid {
            return Ok(());
        }

        // Erase whatever is on the recovery partition currently
        if let Ok(dir) = recovery_path.read_dir() {
            for entry in dir.filter_map(Result::ok) {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        let _ = fs::remove_file(&entry.path());
                    } else if metadata.is_dir() {
                        let _ = fs::remove_dir_all(&entry.path());
                    }
                }
            }
        }

        let casper_data_: String;
        let casper_data: &str = if Path::new("/cdrom/recovery.conf").exists() {
            casper_data_ = ["/cdrom/casper-", cdrom_uuid, "/"].concat();
            &casper_data_
        } else {
            "/cdrom/casper/"
        };

        let casper = ["casper-", &recovery_uuid.id].concat();
        let recovery = ["Recovery-", &recovery_uuid.id].concat();
        if recovery_uuid.id != cdrom_uuid {
            self.chroot
                .command(
                    "rsync",
                    &["-KLavc", "--delete-before", "/cdrom/.disk", "/cdrom/dists", "/cdrom/pool", "/recovery"],
                )
                .run()?;

            self.chroot
                .command("rsync", &["-KLavc", casper_data, &["/recovery/", &casper].concat()])
                .run()?;
        }

        // Create recovery file.
        let recovery_data = format!(
            r#"HOSTNAME={}
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
            config.keyboard_model.as_deref().unwrap_or(""),
            config.keyboard_variant.as_deref().unwrap_or(""),
            efi_partuuid.id,
            recovery_partuuid.id,
            root_uuid,
            luks_uuid,
        );

        // Copy initrd and vmlinuz to EFI partition
        let recovery_path = self.chroot.path.join("recovery/recovery.conf");
        let mut recovery_file = misc::create(&recovery_path)?;
        recovery_file
            .write_all(recovery_data.as_bytes())
            .with_context(|err| format!("failed to write recovery file: {}", err))?;

        let efi_recovery = ["boot/efi/EFI/", recovery.as_str()].concat();
        let efi_initrd = self.chroot.path.join([&efi_recovery, "/initrd.gz"].concat());
        let efi_vmlinuz = self.chroot.path.join([&efi_recovery, "/vmlinuz.efi"].concat());

        fs::create_dir_all(self.chroot.path.join(efi_recovery))
            .with_context(|err| format!("failed to create EFI recovery directories: {}", err))?;

        misc::cp(&[casper_data, "initrd.gz"].concat(), &efi_initrd)?;
        misc::cp(&[casper_data, "vmlinuz.efi"].concat(), &efi_vmlinuz)?;

        // If the NVIDIA DKMS driver is installed, force it to load in the recovery partition
        // This test must not use /proc or /sys for detection since the installer can run inside a
        // chroot where those come from the host environment.
        let has_nvidia = Path::new("/var/lib/dkms/nvidia").exists();

        let rec_entry_data = format!(
            r#"title {0} recovery
linux /EFI/{1}/vmlinuz.efi
initrd /EFI/{1}/initrd.gz
options {2} boot=casper hostname=recovery userfullname=Recovery username=recovery live-media-path=/{3} live-media=/dev/disk/by-partuuid/{4} noprompt {5}
"#,
            name,
            recovery,
            RECOVERY_BOOT_OPTIONS,
            casper,
            recovery_partuuid.id,
            if has_nvidia { "modules_load=nvidia" } else { "" }
        );
        let loader_entries = self.chroot.path.join("boot/efi/loader/entries/");
        if !loader_entries.exists() {
            fs::create_dir_all(&loader_entries)
                .with_context(|err| format!("failed to create EFI loader directories: {}", err))?;
        }

        let rec_entry_path = loader_entries.join([recovery.as_str(), ".conf"].concat());
        let mut rec_entry_file = misc::create(&rec_entry_path)?;
        rec_entry_file
            .write_all(rec_entry_data.as_bytes())
            .with_context(|err| format!("failed to write recovery EFI entry: {}", err))?;
        Ok(())
    }

    pub fn timezone(&self, region: &Region) -> io::Result<()> {
        self.chroot.command("rm", &["/etc/timezone"]).run()?;

        let args: &[&str] = &[];
        self.chroot.command("ln", args).arg(region.path()).arg("/etc/timezone").run()
    }
}
