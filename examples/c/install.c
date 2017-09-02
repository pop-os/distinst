#include <distinst.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>

const char * step_name(DISTINST_STEP step) {
    switch(step) {
    case DISTINST_STEP_PARTITION:
        return "Partition";
    case DISTINST_STEP_FORMAT:
        return "Format";
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

void on_error(const DistinstError * error, void * user_data) {
    printf("Error: %s %s %p\n", step_name(error->step), strerror(error->err), user_data);
}

void on_status(const DistinstStatus * status, void * user_data) {
    printf("Status: %s %d %p\n", step_name(status->step), status->percent, user_data);
}

int main(int argc, char ** argv) {
    DistinstInstaller * installer = distinst_installer_new();
    distinst_installer_on_error(installer, on_error, (void*)0x12C0FFEE);
    distinst_installer_on_status(installer, on_status, (void *)0xDEADBEEF);

    DistinstConfig config = {
        .squashfs = "../../bash/filesystem.squashfs",
        .drive = "/dev/sda",
    };
    distinst_installer_install(installer, &config);

    distinst_installer_destroy(installer);

    return 0;
}
