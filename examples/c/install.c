#include <distinst.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>

const char * level_name (DISTINST_LOG_LEVEL level) {
    switch(level) {
    case DISTINST_LOG_LEVEL_TRACE:
        return "Trace";
    case DISTINST_LOG_LEVEL_DEBUG:
        return "Debug";
    case DISTINST_LOG_LEVEL_INFO:
        return "Info";
    case DISTINST_LOG_LEVEL_WARN:
        return "Warn";
    case DISTINST_LOG_LEVEL_ERROR:
        return "Error";
    default:
        return "Unknown";
    }
}

const char * step_name(DISTINST_STEP step) {
    switch(step) {
    case DISTINST_STEP_INIT:
        return "Initialize";
    case DISTINST_STEP_PARTITION:
        return "Partition";
    case DISTINST_STEP_EXTRACT:
        return "Extract";
    case DISTINST_STEP_CONFIGURE:
        return "Configure";
    case DISTINST_STEP_BOOTLOADER:
        return "Bootloader";
    default:
        return "Unknown";
    }
}

void on_log(DISTINST_LOG_LEVEL level, const char * message, void * user_data) {
    printf("Log: %s %s %p\n", level_name(level), message, user_data);
}

void on_error(const DistinstError * error, void * user_data) {
    printf("Error: %s %s %p\n", step_name(error->step), strerror(error->err), user_data);
}

void on_status(const DistinstStatus * status, void * user_data) {
    printf("Status: %s %d %p\n", step_name(status->step), status->percent, user_data);
}

int main(int argc, char ** argv) {
    if (argc < 2) {
        fprintf(stderr, "not enough arguments\n");
        return 1;
    }

    const char * disk_path = argv[1];

    distinst_log(on_log, (void*)0xFEEEF000);

    DistinstInstaller * installer = distinst_installer_new();
    distinst_installer_on_error(installer, on_error, (void*)0x12C0FFEE);
    distinst_installer_on_status(installer, on_status, (void *)0xDEADBEEF);

    DistinstConfig config = {
        .hostname = "distinst",
        .keyboard = "us",
        .lang = "en_US.UTF-8",
        .squashfs = "../../tests/filesystem.squashfs",
        .remove = "../../tests/filesystem.manifest-remove",
    };

    DistinstDisk * disk = distinst_disk_new(disk_path);
    if (disk == NULL) {
        fprintf(stderr, "could not find %s\n", disk_path);
        return 1;
    }

    DISTINST_PARTITION_TABLE bootloader = distinst_bootloader_detect();

    // Identify the start of disk by sector
    DistinstSector start_sector = {
        .flag = DISTINST_SECTOR_KIND_START,
        .value = 0
    };

    // Identify the end of disk
    DistinstSector end_sector = {
        .flag = DISTINST_SECTOR_KIND_END,
        .value = 0
    };

    uint64_t start, end;
    DistinstPartitionBuilder * builder;
    int result;
    switch (bootloader) {
    case DISTINST_PARTITION_TABLE_MSDOS:
        // Wipes the partition table clean with a brand new MSDOS partition table.
        if (distinst_disk_mklabel(disk, bootloader) != 0) {
            fprintf(stderr, "unable to write MSDOS partition table to %s\n", disk_path);
            return 1;
        }

        // Obtains the start and end values using a human-readable abstraction.
        start = distinst_disk_get_sector(disk, &start_sector);
        end = distinst_disk_get_sector(disk, &end_sector);

        // Adds a newly-created partition builder object to the disk. This object is
        // defined as an EXT4 partition with the `boot` partition flag, and shall be
        // mounted to `/` within the `/etc/fstab` of the installed system.
        builder = distinst_partition_builder_new(start, end, DISTINST_FILE_SYSTEM_TYPE_EXT4);
        distinst_partition_builder_partition_type(builder, DISTINST_PARTITION_TYPE_PRIMARY);
        distinst_partition_builder_flag(builder, DISTINST_PARTITION_FLAG_BOOT);
        distinst_partition_builder_mount(builder, "/");
        result = distinst_disk_add_partition(disk, builder);

        if (result != 0) {
            fprintf (stderr, "unable to add partition to %s\n", disk_path);
            return 1;
        }

        break;
    case DISTINST_PARTITION_TABLE_GPT:
        if (distinst_disk_mklabel(disk, bootloader) != 0) {
            fprintf (stderr, "unable to write GPT partition table to %s\n", disk_path);
            return 1;
        }

        // Sectors may also be constructed using different units of measurements, such as
        // by megabytes and percents. The library author can choose whichever unit makes
        // more sense for their use cases.
        DistinstSector efi_sector = {
            .flag = DISTINST_SECTOR_KIND_MEGABYTE,
            .value = 512
        };

        start = distinst_disk_get_sector(disk, &start_sector);
        end = distinst_disk_get_sector(disk, &efi_sector);

        // Adds a new partitition builder object which is defined to be a FAT partition
        // with the `esp` flag, and shall be mounted to `/boot/efi` after install. This
        // meets the requirement for an EFI partition with an EFI install.
        builder = distinst_partition_builder_new(start, end, DISTINST_FILE_SYSTEM_TYPE_FAT32);
        distinst_partition_builder_partition_type(builder, DISTINST_PARTITION_TYPE_PRIMARY);
        distinst_partition_builder_flag(builder, DISTINST_PARTITION_FLAG_ESP);
        distinst_partition_builder_mount(builder, "/boot/efi");
        result = distinst_disk_add_partition(disk, builder);

        if (result != 0) {
            fprintf (stderr, "unable to add EFI partition to %s\n", disk_path);
            return 1;
        }

        start = distinst_disk_get_sector(disk, &efi_sector);
        end = distinst_disk_get_sector(disk, &end_sector);

        // EFI installs require both an EFI and root partition, so this add a new EXT4
        // partition that is configured to start at the end of the EFI sector, and
        // continue to the end of the disk.
        builder = distinst_partition_builder_new(start, end, DISTINST_FILE_SYSTEM_TYPE_EXT4);
        distinst_partition_builder_partition_type(builder, DISTINST_PARTITION_TYPE_PRIMARY);
        distinst_partition_builder_mount(builder, "/");
        result = distinst_disk_add_partition(disk, builder);

        if (result != 0) {
            fprintf (stderr, "unable to add / partition to %s\n", disk_path);
            return 1;
        }

        break;
    }

    DistinstDisks * disks = distinst_disks_new();
    distinst_disks_push(disks, disk);

    distinst_installer_install(installer, disks, &config);

    distinst_installer_destroy(installer);

    return 0;
}
