#include <distinst.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

void list_devices() {
	// Obtains a Rust-allocated pointer to all the disk information in the system.
    DistinstDisks *disks = distinst_disks_new();
    printf("Found %lu disks on system\n", disks->length);

    // An error occurred if no disks were found.
    if (disks->length == 0) {
		fputs("list: no disks found\n", stderr);
        exit(1);
    }

	// Prints information regarding each partition found on the disk.
    for (int disk = 0; disk < disks->length; disk++) {
        DistinstDisk current_disk = disks->disks[disk];
        printf("Found disk '%s'\n", current_disk.device_path);

        DistinstPartitions parts = current_disk.partitions;
        if (parts.length == 0) {
            printf("no partitions found on '%s'", current_disk.device_path);
            continue;
        }

        for (int part = 0; part < parts.length; part++) {
            DistinstPartition current_part = parts.parts[part];
            printf("\tFound partition '%s'\n", current_part.device_path);
            printf("\t\tstart_sector: %lu\n", current_part.start_sector);
            printf("\t\tend_sector:   %lu\n", current_part.end_sector);
            printf("\t\tfilesystem:   %s\n", strfilesys(current_part.filesystem));
        }
    }

	/// Unallocates the disks object that was created w/ Rust.
    distinst_disks_destroy(disks);
}

int main(int argc, char **argv) {
	if (geteuid() != 0) {
		puts("root is required to poll disk information (for now)");
		exit(1);
	}

    list_devices();
}
