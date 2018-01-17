#include <distinst.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

void list_devices() {
    DistinstDisks *disks = distinst_disks_new();
    if (disks->length == 0) {
        fprintf(stderr, "no disks found\n");
        exit(1);
    }

    printf("Found %lu disks on system\n", disks->length);
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
}

int main(int argc, char **argv) {
    list_devices();
}