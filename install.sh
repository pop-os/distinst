#!/usr/bin/env bash

set -ex

SQUASHFS="/home/jeremy/Projects/pop/iso/build/18.04/iso/casper/filesystem.squashfs"
HOSTNAME="pop-os"
KBD="us"
REMOVE="/home/jeremy/Projects/pop/iso/build/18.04/iso/casper/filesystem.manifest-remove"
DISK="/dev/disk/by-id/usb-NORELSYS_106X_0123456789ABCDE-0:0"

make
sudo make install
sudo distinst \
    --squashfs "${SQUASHFS}" \
    --hostname "${HOSTNAME}" \
    --keyboard "${KBD}" \
    --lang "${LANG}" \
    --remove "${REMOVE}" \
    --block "${DISK}" \
    "$@"
