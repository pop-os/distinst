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
   }
}

int main(int argc, char **argv) {
   list_devices();
}