using Distinst;

public static string level_name (LogLevel level) {
    switch(level) {
    case LogLevel.TRACE:
        return "Trace";
    case LogLevel.DEBUG:
        return "Debug";
    case LogLevel.INFO:
        return "Info";
    case LogLevel.WARN:
        return "Warn";
    case LogLevel.ERROR:
        return "Error";
    default:
        return "Unknown";
    }
}

public static string step_name (Step step) {
    switch(step) {
    case Step.INIT:
        return "Initialize";
    case Step.PARTITION:
        return "Partition";
    case Step.EXTRACT:
        return "Extract";
    case Step.CONFIGURE:
        return "Configure";
    case Step.BOOTLOADER:
        return "Bootloader";
    default:
        return "Unknown";
    }
}

public static int main (string[] args) {
    if (args.length < 2) {
        stderr.printf("not enough arguments\n");
        return 1;
    }

    var disk_path = args[1];

    var user_data = 0x12C0FFEE;

    Distinst.log((level, message) => {
        stderr.printf ("Log: %s %s %X\r\n", level_name (level), message, user_data);
    });

    var installer = new Installer ();

    installer.on_error((error) => {
        stderr.printf ("Error: %s %s %X\r\n", step_name (error.step), strerror (error.err), user_data);
    });

    installer.on_status((status) => {
        stderr.printf ("Status: %s %d %X\r\n", step_name (status.step), status.percent, user_data);
    });

    var config = Config ();

    config.hostname = "distinst";
    config.keyboard_layout = "us";
    config.lang = "en_US.UTF-8";
    config.squashfs = "../../tests/filesystem.squashfs";
    config.remove = "../../tests/filesystem.manifest-remove";

    Disk disk = new Disk (disk_path);
    if (disk == null) {
        stderr.printf("could not find %s\n", disk_path);
        return 1;
    }

    // Obtains the preferred partition table based on what the system is currently loaded with.
    // EFI partitions will need to have both an EFI partition with an `esp` flag, and a root
    // partition; whereas MBR-based installations only require a root partition.
    PartitionTable bootloader = bootloader_detect ();

    // Identify the start of disk by sector
    var start_sector = Sector() {
        flag = SectorKind.START,
        value = 0
    };

    // Identify the end of disk
    var end_sector = Sector() {
        flag = SectorKind.END,
        value = 0
    };

    switch (bootloader) {
        case PartitionTable.MSDOS:
            // Wipes the partition table clean with a brand new MSDOS partition table.
            if (disk.mklabel (bootloader) != 0) {
                stderr.printf("unable to write MSDOS partition table to %s\n", disk_path);
                return 1;
            }

            // Obtains the start and end values using a human-readable abstraction.
            var start = disk.get_sector (ref start_sector);
            var end = disk.get_sector (ref end_sector);

            // Adds a newly-created partition builder object to the disk. This object is
            // defined as an EXT4 partition with the `boot` partition flag, and shall be
            // mounted to `/` within the `/etc/fstab` of the installed system.
            int result = disk.add_partition(
                new PartitionBuilder (start, end, FileSystem.EXT4)
                    .partition_type (PartitionType.PRIMARY)
                    .flag (PartitionFlag.BOOT)
                    .mount ("/")
            );

            if (result != 0) {
                stderr.printf ("unable to add partition to %s\n", disk_path);
                return 1;
            }

            break;
        case PartitionTable.GPT:
            stderr.printf("mklabel\n");
            if (disk.mklabel (bootloader) != 0) {
                stderr.printf ("unable to write GPT partition table to %s\n", disk_path);
                return 1;
            }

            // Sectors may also be constructed using different units of measurements, such as
            // by megabytes and percents. The library author can choose whichever unit makes
            // more sense for their use cases.
            var efi_sector = Sector() {
                flag = SectorKind.MEGABYTE,
                value = 512
            };

            var start = disk.get_sector (ref start_sector);
            var end = disk.get_sector (ref efi_sector);

            // Adds a new partitition builder object which is defined to be a FAT partition
            // with the `esp` flag, and shall be mounted to `/boot/efi` after install. This
            // meets the requirement for an EFI partition with an EFI install.
            int result = disk.add_partition(
                new PartitionBuilder (start, end, FileSystem.FAT32)
                    .partition_type (PartitionType.PRIMARY)
                    .flag (PartitionFlag.ESP)
                    .mount ("/boot/efi")
            );

            if (result != 0) {
                stderr.printf ("unable to add EFI partition to %s\n", disk_path);
                return 1;
            }

            start = disk.get_sector (ref efi_sector);
            end = disk.get_sector (ref end_sector);

            // EFI installs require both an EFI and root partition, so this add a new EXT4
            // partition that is configured to start at the end of the EFI sector, and
            // continue to the end of the disk.
            result = disk.add_partition (
                new PartitionBuilder (start, end, FileSystem.EXT4)
                    .partition_type (PartitionType.PRIMARY)
                    .mount ("/")
            );

            if (result != 0) {
                stderr.printf ("unable to add / partition to %s\n", disk_path);
                return 1;
            }

            break;
    }

    // Each disk that will have changes made to it should be added to a Disks object. This
    // object will be passed to the install method, and used as a blueprint for how changes
    // to each disk should be made, and where critical partitions are located.
    Disks disks = new Disks ();
    disks.push ((owned) disk);

    installer.install ((owned) disks, config);

    return 0;
}
