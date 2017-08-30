#include <distinst.h>
#include <stdio.h>

void on_status(uint32_t status) {
    printf("Status: %d\n", status);
}

int main(int argc, char ** argv) {
    Installer * installer = installer_new();

    installer_on_status(installer, on_status);

    installer_emit_status(installer, 10);

    installer_destroy(installer);

    return 0;
}
